use std::io::Error;

use clap::Parser;
use serde::{Serialize, Deserialize};
use tokio_postgres::Client;

use avtor_core::models::users::{insert_user, find_super_user, UserDto, create_super_user};

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

async fn _create_super_user(client: Client, user_dto: &UserDto) {
    let this_insert_user = insert_user(&client);
    let this_find_super_user = find_super_user(&client);
    let _ = create_super_user(this_find_super_user, this_insert_user, user_dto).await;
    ()
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


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    match args.op.as_str() {
        "hello" => Ok(println!("hello")),
        "create_super_user" => {
            match args.path {
                None => Ok(println!("Credentials file path required for {} operation", args.op)),
                Some(path) => {
                    let file = std::fs::File::open(path)?; 
                    let yamlSuperUser: YamlSuperUser = serde_yaml::from_reader(file)?;
                    Ok(println!("put parsing logic here"))
                }
            }
        }
        _ => Ok(println!("operation {} not recognized", args.op))
    } 
}
