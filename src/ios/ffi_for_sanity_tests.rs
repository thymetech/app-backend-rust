use super::ios_interface::cstring_to_str;
use core_foundation::{
    base::TCFType,
    string::{CFString, CFStringRef},
};
use libc::c_char;
use mpsc::Receiver;
use std::{
    sync::mpsc::{self, Sender},
    thread,
};
use log::*;


// Expose an interface for apps (for now only iOS) to test that general FFI is working as expected.
// i.e. assumptions on which the actual FFI interface relies.
// TODO can the c headers for this be generated in a separate file? Needs adjustments in script to generate lib and framework too.

#[repr(C)]
pub struct FFIParameterStruct {
    my_int: i32,
    my_str: *const c_char, // TODO use CFStringRef here too?
    my_nested: FFINestedParameterStruct,
}

#[repr(C)]
pub struct FFINestedParameterStruct {
    my_u8: u8,
}

#[derive(Debug)]
struct MyStruct {
    my_int: i32,
    my_str: String,
    my_u8: u8,
}
#[repr(u8)]
#[derive(Debug, Clone)]
pub enum CoreLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CoreLogMessage {
    level: CoreLogLevel,
    text: CFStringRef,
    time: i64,
}


impl From<CoreLogMessageThreadSafe> for CoreLogMessage{
    fn from(lts: CoreLogMessageThreadSafe) -> Self {
        let cf_string = CFString::new(&lts.text);
        let cf_string_ref = cf_string.as_concrete_TypeRef();
        ::std::mem::forget(cf_string);
        CoreLogMessage{
            level: lts.level,
            text: cf_string_ref,
            time: lts.time
        }
    }
}

pub struct CoreLogMessageThreadSafe {
    //TODO: hide fields
    pub level: CoreLogLevel,
    pub text: String,
    pub time: i64,
}




#[no_mangle]
pub unsafe extern "C" fn pass_struct(par: *const FFIParameterStruct) -> i32 {
    let my_str = cstring_to_str(&(*par).my_str).unwrap();

    let my_struct = MyStruct {
        my_int: (*par).my_int,
        my_str: my_str.to_owned(),
        my_u8: (*par).my_nested.my_u8,
    };

    info!("Received struct from iOS: {:?}", my_struct);

    1
}

#[repr(C)]
pub struct FFIReturnStruct {
    my_int: i32,
    my_str: CFStringRef,
    my_nested: FFINestedReturnStruct,
}

#[repr(C)]
pub struct FFINestedReturnStruct {
    my_u8: u8,
}

#[no_mangle]
pub unsafe extern "C" fn return_struct() -> FFIReturnStruct {
    let my_str = "hi!";
    let cf_string = CFString::new(&my_str.to_owned());
    let cf_string_ref = cf_string.as_concrete_TypeRef();

    ::std::mem::forget(cf_string);

    FFIReturnStruct {
        my_int: 123,
        my_str: cf_string_ref,
        my_nested: FFINestedReturnStruct { my_u8: 255 },
    }
}

#[no_mangle]
pub unsafe extern "C" fn pass_and_return_struct(par: *const FFIParameterStruct) -> FFIReturnStruct {
    let my_str = cstring_to_str(&(*par).my_str).unwrap();
    let cf_string = CFString::new(&my_str.to_owned());
    let cf_string_ref = cf_string.as_concrete_TypeRef();

    ::std::mem::forget(cf_string);

    // TODO use CFStringRef in par?

    FFIReturnStruct {
        my_int: (*par).my_int,
        my_str: cf_string_ref,
        my_nested: FFINestedReturnStruct {
            my_u8: (*par).my_nested.my_u8,
        },
    }
}

pub trait Callback {
    fn call(&self, my_int: i32, my_bool: bool, my_str: CFStringRef);
}

impl Callback for unsafe extern "C" fn(i32, bool, CFStringRef) {
    fn call(&self, a_number: i32, a_boolean: bool, my_str: CFStringRef) {
        unsafe {
            self(a_number, a_boolean, my_str);
        }
    }
}


pub trait LogCallback {
    fn call(&self, log_message: CoreLogMessage);
}

impl LogCallback for unsafe extern "C" fn(CoreLogMessage) {
    fn call(&self, log_message: CoreLogMessage) {
        unsafe {
            self(log_message);
        }
    }
}



#[no_mangle]
pub extern "C" fn call_callback(callback: unsafe extern "C" fn(i32, bool, CFStringRef)) -> i32 {
    let cf_string = CFString::new(&"hi!".to_owned());
    let cf_string_ref = cf_string.as_concrete_TypeRef();

    callback.call(123, false, cf_string_ref);
    1
}

pub static mut SENDER: Option<Sender<String>> = None;
pub static mut LOG_SENDER: Option<Sender<CoreLogMessageThreadSafe>> = None;

#[no_mangle]
pub unsafe extern "C" fn register_callback(
    callback: unsafe extern "C" fn(i32, bool, CFStringRef),
) -> i32 {
    register_callback_internal(Box::new(callback));
    1
}

#[no_mangle]
pub unsafe extern "C" fn register_log_callback(
    log_callback: unsafe extern "C" fn(CoreLogMessage),
) -> i32 {
    register_log_callback_internal(Box::new(log_callback));
    2
}

#[no_mangle]
pub unsafe extern "C" fn trigger_callback(my_str: *const c_char) -> i32 {
    let str = cstring_to_str(&my_str).unwrap();
    match &SENDER {
        // Push element to SENDER.
        Some(s) => {
            s.send(str.to_owned()).expect("Couldn't send");
            1
        }

        None => {
            warn!("No callback registered");
            0
        }
    }
}

fn register_callback_internal(callback: Box<dyn Callback>) {
    // Make callback implement Send (marker for thread safe, basically) https://doc.rust-lang.org/std/marker/trait.Send.html
    let my_callback =
        unsafe { std::mem::transmute::<Box<dyn Callback>, Box<dyn Callback + Send>>(callback) };

    // Create channel
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();

    // Save the sender in a static variable, which will be used to push elements to the callback
    unsafe {
        SENDER = Some(tx);
    }

    // Thread waits for elements pushed to SENDER and calls the callback
    thread::spawn(move || {
        for str in rx.iter() {
            let cf_string = CFString::new(&str.to_owned());
            let cf_string_ref = cf_string.as_concrete_TypeRef();
            // For convenience, pass around only the string and hardcode the other 2 parameters.
            my_callback.call(1, true, cf_string_ref)
        }
    });
}

fn register_log_callback_internal(callback: Box<dyn LogCallback>) {
    // Make callback implement Send (marker for thread safe, basically) https://doc.rust-lang.org/std/marker/trait.Send.html
    let log_callback =
        unsafe { std::mem::transmute::<Box<dyn LogCallback>, Box<dyn LogCallback + Send>>(callback) };

    // Create channel
    let (tx, rx): (Sender<CoreLogMessageThreadSafe>, Receiver<CoreLogMessageThreadSafe>) = mpsc::channel();

    // Save the sender in a static variable, which will be used to push elements to the callback
    unsafe {
        LOG_SENDER = Some(tx);
    }

    // Thread waits for elements pushed to SENDER and calls the callback
    thread::spawn(move || {
        for log_entry in rx.iter() {
             log_callback.call(log_entry.into());
        }
    });
}

#[no_mangle]
pub unsafe extern "C" fn trigger_logging_macros() -> i32 {
    debug!(target: "test_events", "CoEpi debug");
    trace!(target: "test_events", "CoEpi trace");
    info!(target: "test_events", "CoEpi info");
    warn!(target: "test_events", "CoEpi warn");
    error!(target: "test_events", "CoEpi error");
    
    1
}