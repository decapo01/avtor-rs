use tokio_postgres::Client;

use super::migration_01;

pub async fn run_migration_up(client: &mut Client) -> Result<(), anyhow::Error> {
    migration_01::run_migration(client).await
}
