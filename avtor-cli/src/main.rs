use std::{io::Error, str::FromStr};

use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio_postgres::{tls::NoTlsStream, Client, Connection, NoTls, Socket};

use avtor_core::models::users::{
    create_super_user, find_account_by_id, find_super_user, insert_account, insert_user,
    AccountDto, CreateSuperUserError, UserDto,
};

pub mod migrations;
use migrations::migration_01::run_migration_up;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(long)]
    op: String,

    #[clap(long)]
    other: Option<String>,

    path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct YamlSuperUser {
    pub username: String,
    pub password: String,
}

async fn _create_super_user(
    client: &mut Client,
    user_dto: &UserDto,
    account_dto: &AccountDto,
) -> Result<(), CreateSuperUserError> {
    // todo: map err
    let trans = client.transaction().await.unwrap();
    let this_insert_user = insert_user(&trans);
    let this_find_super_user = find_super_user(&trans);
    let insert_account_setup = insert_account(&trans);
    let find_account_by_id_setup = find_account_by_id(&trans);
    let r = create_super_user(
        this_find_super_user,
        this_insert_user,
        insert_account_setup,
        find_account_by_id_setup,
        user_dto,
        account_dto,
    )
    .await;
    trans.commit().await;
    r
}

#[derive(Deserialize, Debug)]
pub struct EnvConfig {
    pub db_host: String,
    pub db_port: String,
    pub db_user: String,
    pub db_pass: String,
    pub db_name: Option<String>,
    pub main_account_id: String,
    pub main_account_name: String,
    pub super_user_username: String,
    pub super_user_password: String,
}

// todo: move into package
pub fn conn_str_from_config(config: &EnvConfig) -> String {
    format!(
        "postgres://{user}:{password}@{host}:{port}/{db}",
        user = config.db_user,
        password = config.db_pass,
        host = config.db_host,
        port = config.db_port,
        db = config.db_name.to_owned().unwrap_or("postgres".to_string()),
    )
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    let env_config = envy::from_env::<EnvConfig>()?;
    let conn_str = conn_str_from_config(&env_config);
    let (mut client, conn) = tokio_postgres::connect(&conn_str, NoTls).await?;
    match args.op.as_str() {
        "hello" => Ok(println!("hello")),
        "run_migrations" => {
            tokio::spawn(async move {
                if let Err(e) = conn.await {
                    eprintln!("conn error: {}", e);
                }
            });
            migrations::run_migrations::run_migration_up(&mut client).await
        }
        "create_super_user" => match args.path {
            None => Ok(println!(
                "Credentials file path required for {} operation",
                args.op
            )),
            Some(path) => {
                let file = std::fs::File::open(path)?;
                let yamlSuperUser: YamlSuperUser = serde_yaml::from_reader(file)?;
                let user_dto = UserDto {
                    id: uuid::Uuid::new_v4(),
                    username: env_config.super_user_username,
                    password: env_config.super_user_password,
                    roles: "super_user".to_string(),
                    account_id: uuid::Uuid::from_str(env_config.main_account_id.as_str())?,
                };
                let account_dto = AccountDto {
                    id: uuid::Uuid::from_str(env_config.main_account_id.as_str())?,
                    name: env_config.main_account_name,
                };
                Ok(println!("put parsing logic here"))
            }
        },
        _ => Ok(println!("operation {} not recognized", args.op)),
    }
}
