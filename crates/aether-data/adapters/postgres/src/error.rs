use crate::DataLayerError;

pub(crate) fn postgres_error(error: impl std::fmt::Display) -> DataLayerError {
    DataLayerError::postgres(error)
}

fn postgres_sqlx_error(error: sqlx::Error) -> DataLayerError {
    let sqlstate = error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned());
    match sqlstate {
        Some(code) => DataLayerError::Postgres(format!("{error} (SQLSTATE {code})")),
        None => DataLayerError::postgres(error),
    }
}

pub(crate) trait SqlxResultExt<T> {
    fn map_postgres_err(self) -> Result<T, DataLayerError>;
}

impl<T> SqlxResultExt<T> for Result<T, sqlx::Error> {
    fn map_postgres_err(self) -> Result<T, DataLayerError> {
        self.map_err(postgres_sqlx_error)
    }
}
