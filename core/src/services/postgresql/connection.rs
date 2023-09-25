use std::sync::Arc;

use async_trait::async_trait;
use bb8;
use tokio::sync::OnceCell;
use tokio_postgres::Client;
use tokio_postgres::Config;
use tokio_postgres::Statement;

use crate::*;

pub struct CustomPostgresqlConnection {
    client: OnceCell<Arc<Client>>,
    config: Config,

    table: String,
    key_field: String,
    value_field: String,

    statement_get: OnceCell<Statement>,
    statement_set: OnceCell<Statement>,
    statement_del: OnceCell<Statement>,
}

impl CustomPostgresqlConnection {
    async fn get_client(&self) -> Result<&Client> {
        self.client
            .get_or_try_init(|| async {
                // TODO: add tls support.
                let (client, conn) = self.config.connect(tokio_postgres::NoTls).await?;

                // The connection object performs the actual communication with the database,
                // so spawn it off to run on its own.
                tokio::spawn(async move {
                    if let Err(e) = conn.await {
                        eprintln!("postgresql connection error: {}", e);
                    }
                });

                Ok(Arc::new(client))
            })
            .await
            .map(|v| v.as_ref())
    }

    async fn get_statement_prepared(&mut self) -> Result<Statement> {
        let query = format!(
            "SELECT {} FROM {} WHERE {} = $1 LIMIT 1",
            self.value_field, self.table, self.key_field
        );
        self.statement_get
            .get_or_try_init(|| async {
                self.get_client()
                    .await?
                    .prepare(&query)
                    .await
                    .map_err(Error::from)
            })
            .await
            .map_err(Error::from);
    }

    async fn set_statement_prepared(&mut self) -> Result<Statement> {
        let query = format!(
            "INSERT INTO `{}` (`{}`, `{}`) 
            VALUES (:path, :value) 
            ON DUPLICATE KEY UPDATE `{}` = VALUES({})",
            self.table, self.key_field, self.value_field, self.value_field, self.value_field
        );
        self.statement_set
            .get_or_try_init(|| async {
                self.get_client()
                    .await?
                    .prepare(&query)
                    .await
                    .map_err(Error::from)
            })
            .await
            .map_err(Error::from);
    }

    async fn del_statement_prepared(&mut self) -> Result<Statement> {
        let query = format!(
            "INSERT INTO `{}` (`{}`, `{}`) 
            VALUES (:path, :value) 
            ON DUPLICATE KEY UPDATE `{}` = VALUES({})",
            self.table, self.key_field, self.value_field, self.value_field, self.value_field
        );
        self.statement_set
            .get_or_try_init(|| async {
                self.get_client()
                    .await?
                    .prepare(&query)
                    .await
                    .map_err(Error::from)
            })
            .await
            .map_err(Error::from);
    }
}

pub struct CustomPostgresqlConnectionManager {
    config: Config,

    table: String,
    key_field: String,
    value_field: String,

    statement_get: OnceCell<Statement>,
    statement_set: OnceCell<Statement>,
    statement_del: OnceCell<Statement>,
}

#[async_trait]
impl bb8::ManageConnection for CustomPostgresqlConnection {
    type Connection = CustomPostgresqlConnection;
    type Error = Error;

    async fn connect(&self) -> std::result::Result<Self::Connection, Self::Error> {
        let client = CustomPostgresqlConnection {
            client: OnceCell::new(),
            config: self.config.clone(),

            table: self.table.clone(),
            key_field: self.key_field.clone(),
            value_field: self.value_field.clone(),

            statement_get: OnceCell::new(),
            statement_set: OnceCell::new(),
            statement_del: OnceCell::new(),
        };

        match client.get_statement_prepared().await {
            Ok(_) => (),
            Err(e) => return Err(e),
        }
        match client.set_statement_prepared().await {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        match client.del_statement_prepared().await {
            Ok(_) => (),
            Err(e) => return Err(e),
        }
        Ok(client)
    }

    async fn is_valid(&self, conn: &mut Self::Connection) -> std::result::Result<(), Self::Error> {
        conn.get_client().await?.simple_query("").await.map(|_| ())
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.get_client().await?.is_closed()
    }
}
