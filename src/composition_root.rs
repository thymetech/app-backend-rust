use crate::networking::{TcnApi, TcnApiImpl};
use crate::reports_updater::{
    ObservedTcnProcessor, ObservedTcnProcessorImpl, ReportsUpdater, TcnDao, TcnDaoImpl, TcnMatcher,
    TcnMatcherRayon,
};
use crate::{
    errors::ServicesError,
    expect_log,
    preferences::{Database, Preferences, PreferencesDao, PreferencesImpl},
    reporting::{
        memo::{MemoMapper, MemoMapperImpl},
        symptom_inputs::{SymptomInputs, SymptomInputsSubmitterImpl},
        symptom_inputs_manager::{
            SymptomInputsManagerImpl, SymptomInputsProcessor, SymptomInputsProcessorImpl,
        },
    },
    tcn_ext::tcn_keys::{TcnKeys, TcnKeysImpl},
};
use log::*;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use rusqlite::Connection;
use std::sync::Arc;

#[allow(dead_code)]
pub struct CompositionRoot<'a, A, B, C, D, F, G, H, I>
where
    A: Preferences,
    B: TcnDao,
    C: TcnMatcher,
    D: TcnApi,
    F: SymptomInputsProcessor,
    G: ObservedTcnProcessor,
    H: MemoMapper,
    I: TcnKeys,
{
    pub api: &'a D,
    pub reports_updater: ReportsUpdater<'a, A, B, C, D, H>,
    pub symptom_inputs_processor: F,
    pub observed_tcn_processor: G,
    pub tcn_keys: Arc<I>,
}

pub static COMP_ROOT: OnceCell<
    CompositionRoot<
        PreferencesImpl,
        TcnDaoImpl,
        TcnMatcherRayon,
        TcnApiImpl,
        SymptomInputsProcessorImpl<
            SymptomInputsManagerImpl<
                SymptomInputsSubmitterImpl<
                    MemoMapperImpl,
                    TcnKeysImpl<PreferencesImpl>,
                    TcnApiImpl,
                >,
            >,
        >,
        ObservedTcnProcessorImpl<TcnDaoImpl>,
        MemoMapperImpl,
        TcnKeysImpl<PreferencesImpl>,
    >,
> = OnceCell::new();

pub fn bootstrap(db_path: &str) -> Result<(), ServicesError> {
    info!("Bootstrapping with db path: {:?}", db_path);

    let sqlite_path = format!("{}/db.sqlite", db_path);
    debug!("Sqlite path: {:?}", sqlite_path);

    if let Err(_) = COMP_ROOT.set(create_comp_root(sqlite_path.as_ref())) {
        return Err(ServicesError::General(
            "Couldn't initialize dependencies".to_owned(),
        ));
    };

    Ok(())
}

pub fn dependencies() -> &'static CompositionRoot<
    'static,
    PreferencesImpl,
    TcnDaoImpl,
    TcnMatcherRayon,
    TcnApiImpl,
    SymptomInputsProcessorImpl<
        SymptomInputsManagerImpl<
            SymptomInputsSubmitterImpl<
                'static,
                MemoMapperImpl,
                TcnKeysImpl<PreferencesImpl>,
                TcnApiImpl,
            >,
        >,
    >,
    ObservedTcnProcessorImpl<TcnDaoImpl>,
    MemoMapperImpl,
    TcnKeysImpl<PreferencesImpl>,
> {
    let res = COMP_ROOT
        .get()
        .ok_or(ServicesError::General("COMP_ROOT not set".to_owned()));

    // Note that the error message here is unlikely to appear on Android, as if COMP_ROOT is not set
    // most likely bootstrap hasn't been executed (which initializes the logger)
    expect_log!(res, "COMP_ROOT not set. Maybe app didn't call bootstrap?")
}

fn create_comp_root(
    sqlite_path: &str,
) -> CompositionRoot<
    'static,
    PreferencesImpl,
    TcnDaoImpl,
    TcnMatcherRayon,
    TcnApiImpl,
    SymptomInputsProcessorImpl<
        SymptomInputsManagerImpl<
            SymptomInputsSubmitterImpl<
                'static,
                MemoMapperImpl,
                TcnKeysImpl<PreferencesImpl>,
                TcnApiImpl,
            >,
        >,
    >,
    ObservedTcnProcessorImpl<TcnDaoImpl>,
    MemoMapperImpl,
    TcnKeysImpl<PreferencesImpl>,
> {
    let api = &TcnApiImpl {};

    let connection_res = Connection::open(sqlite_path);
    let connection = expect_log!(connection_res, "Couldn't create database!");
    let database = Arc::new(Database::new(connection));

    let preferences_dao = PreferencesDao::new(database.clone());
    let preferences = Arc::new(PreferencesImpl {
        dao: preferences_dao,
    });

    let memo_mapper = &MemoMapperImpl {};

    let tcn_keys = Arc::new(TcnKeysImpl {
        preferences: preferences.clone(),
    });

    let symptom_inputs_submitter = SymptomInputsSubmitterImpl {
        memo_mapper,
        tcn_keys: tcn_keys.clone(),
        api,
    };

    let tcn_dao = Arc::new(TcnDaoImpl::new(database.clone()));

    CompositionRoot {
        api,
        reports_updater: ReportsUpdater {
            preferences: preferences.clone(),
            tcn_dao: tcn_dao.clone(),
            tcn_matcher: TcnMatcherRayon {},
            api,
            memo_mapper,
        },
        symptom_inputs_processor: SymptomInputsProcessorImpl {
            inputs_manager: SymptomInputsManagerImpl {
                inputs: Arc::new(RwLock::new(SymptomInputs::default())),
                inputs_submitter: symptom_inputs_submitter,
            },
        },
        observed_tcn_processor: ObservedTcnProcessorImpl {
            tcn_dao: tcn_dao.clone(),
        },
        tcn_keys: tcn_keys.clone(),
    }
}
