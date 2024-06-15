use std::path::PathBuf;
use tokio::signal;
use tracing::info;
use tracing::level_filters::LevelFilter;

use crate::api::{start_graphql_server, StartGraphqlServerError};
use crate::database::postgres::SetupPostgresError;
use crate::generator::event_callback_registry::EventCallbackRegistry;
use crate::indexer::no_code::{setup_no_code, SetupNoCodeError};
use crate::indexer::start::{start_indexing, StartIndexingError, StartIndexingSettings};
use crate::manifest::yaml::{read_manifest, ProjectType, ReadManifestError};
use crate::{setup_logger, setup_postgres, GraphQLServerDetails};

pub struct IndexingDetails {
    pub registry: EventCallbackRegistry,
    pub settings: StartIndexingSettings,
}

pub struct StartDetails {
    pub manifest_path: PathBuf,
    pub indexing_details: Option<IndexingDetails>,
    pub graphql_server: Option<GraphQLServerDetails>,
}

#[derive(thiserror::Error, Debug)]
pub enum StartRindexerError {
    #[error("Could not read manifest: {0}")]
    CouldNotReadManifest(ReadManifestError),

    #[error("Could not start graphql error {0}")]
    CouldNotStartGraphqlServer(StartGraphqlServerError),

    #[error("Could not setup postgres: {0}")]
    SetupPostgresError(SetupPostgresError),

    #[error("Could not start indexing: {0}")]
    CouldNotStartIndexing(StartIndexingError),
}

pub async fn start_rindexer(details: StartDetails) -> Result<(), StartRindexerError> {
    let manifest =
        read_manifest(&details.manifest_path).map_err(StartRindexerError::CouldNotReadManifest)?;

    if manifest.project_type != ProjectType::NoCode {
        setup_logger(LevelFilter::INFO);
        info!("Starting rindexer rust project");
    }

    if let Some(graphql_server) = details.graphql_server {
        let _ = start_graphql_server(&manifest.indexers, graphql_server.settings)
            .map_err(StartRindexerError::CouldNotStartGraphqlServer)?;
        if details.indexing_details.is_none() {
            signal::ctrl_c().await.expect("failed to listen for event");
            return Ok(());
        }
    }

    if let Some(indexing_details) = details.indexing_details {
        // setup postgres is already called in no-code startup
        if manifest.project_type != ProjectType::NoCode && manifest.storage.postgres_enabled() {
            setup_postgres(&manifest)
                .await
                .map_err(StartRindexerError::SetupPostgresError)?;
        }

        start_indexing(
            &manifest,
            indexing_details.registry.complete(),
            indexing_details.settings,
        )
        .await
        .map_err(StartRindexerError::CouldNotStartIndexing)?;
    }

    Ok(())
}

pub struct StartNoCodeDetails {
    pub manifest_path: PathBuf,
    pub indexing_settings: Option<StartIndexingSettings>,
    pub graphql_server: Option<GraphQLServerDetails>,
}

#[derive(thiserror::Error, Debug)]
pub enum StartRindexerNoCode {
    #[error("{0}")]
    StartRindexerError(StartRindexerError),

    #[error("{0}")]
    SetupNoCodeError(SetupNoCodeError),
}

pub async fn start_rindexer_no_code(
    details: StartNoCodeDetails,
) -> Result<(), StartRindexerNoCode> {
    let start_details = setup_no_code(details)
        .await
        .map_err(StartRindexerNoCode::SetupNoCodeError)?;

    start_rindexer(start_details)
        .await
        .map_err(StartRindexerNoCode::StartRindexerError)
}