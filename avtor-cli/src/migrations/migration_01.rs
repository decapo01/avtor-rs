use std::{any, future::Future};

use avtor_core::models::migrations::{
    self, create, find_one, Migration, MigrationCriteria, MigrationId, MyTimeStamp,
};
use chrono::{Local, NaiveDateTime, Utc};
use tokio_postgres::{Client, Transaction};
use uuid::Uuid;

const up: &'static str = "
create table if not exists accounts (
  id uuid not null primary key,
  name varchar(255),
  created_on timestamp default current_timestamp
);";

const up_02: &'static str = "
CREATE table if not exists users (
  id uuid not null primary key,
  username varchar(255) not null,
  password varchar(255) not null,
  roles text not null,
  account_id uuid not null references accounts(id),
  created_on timestamp default current_timestamp 
);";

const down: &'static str = "
drop table users;
drop table accounts;";

pub async fn run_migration_up<'a>(client: &Transaction<'a>) -> Result<(), anyhow::Error> {
    let stmt = client.prepare(up).await?;
    let stmt2 = client.prepare(up_02).await?;
    client.execute(&stmt, &[]).await?;
    client.execute(&stmt2, &[]).await?;
    Ok(())
}

pub async fn run_migrations_down<'a>(client: &Transaction<'a>) -> Result<(), ()> {
    let stmt = client.prepare(down).await.map_err(|_| ())?;
    client.execute(&stmt, &[]).await.map_err(|_| ())?;
    Ok(println!("running migrations down"))
}

pub async fn run_migration(client: &mut Client) -> Result<(), anyhow::Error> {
    let crit = vec![MigrationCriteria::SeqOrderEq(1)];
    let trans_builder = client.build_transaction();
    let trans = trans_builder.start().await?;
    let mig = migrations::find_one(&trans)(crit).await?;
    let r = match mig {
        Some(_) => Ok(()),
        None => match run_migration_up(&trans).await {
            Err(_) => {
                println!("Migrations 1 Up failed running downs");
                match run_migrations_down(&trans).await {
                    Ok(_) => {
                        println!("Migration 1 down ran without error");
                        Ok(())
                    }
                    Err(_) => Ok(()),
                }
            }
            _ => {
                let new_migration = Migration {
                    id: Uuid::new_v4(),
                    name: "migration_01".to_string(),
                    seq_order: 1,
                    up: format!(
                        "{}
                        {}",
                        up, up_02
                    ),
                    down: down.to_string(),
                    applied_on: Utc::now().naive_utc(),
                };
                create(&trans)(new_migration).await?;
                println!("Migration 1 ran without error");
                Ok(())
            }
        },
    };
    trans.commit().await?;
    r
}
