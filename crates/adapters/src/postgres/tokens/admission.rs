use super::*;
use darkhorse_application::introspection_admission::{Authenticate, Caller, Credentials};
impl Authenticate for PostgresStore {
    async fn authenticate_introspection(&self, credentials: Credentials) -> Result<(), Error> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        authority::lock(&mut tx).await.map_err(storage)?;
        match credentials.caller {
            Caller::Client(client) => reads::client(&mut tx, client, credentials.secret).await?,
            Caller::Resource(resource) => {
                resources::authenticate(
                    &mut tx,
                    &darkhorse_application::resource_servers::Probe {
                        resource,
                        secret: credentials.secret,
                        token: None,
                    },
                )
                .await?;
            }
        }
        tx.commit().await.map_err(storage)?;
        Ok(())
    }
}
