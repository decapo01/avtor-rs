use std::str::FromStr;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use futures::{future::BoxFuture, stream::BoxStream, Future, Stream, StreamExt};
use postgres_derive::{FromSql, ToSql};
use tokio_postgres::{Client, Row, Transaction};
use uuid::Uuid;

use crate::postgres_common::core::{
    entity, insert, select, select_all, select_raw, QueryCondition,
};

use super::common::field_names_without_id;

pub async fn blah() {
    let secret = "blah";
    let client = stripe::Client::new(secret);
    let customer = stripe::Customer::create(
        &client,
        stripe::CreateCustomer {
            name: Some("blah"),
            email: Some("sldkjfl"),
            description: Some(""),
            address: todo!(),
            balance: todo!(),
            cash_balance: todo!(),
            coupon: todo!(),
            expand: todo!(),
            invoice_prefix: todo!(),
            invoice_settings: todo!(),
            metadata: todo!(),
            next_invoice_sequence: todo!(),
            payment_method: todo!(),
            phone: todo!(),
            preferred_locales: todo!(),
            promotion_code: todo!(),
            shipping: todo!(),
            source: todo!(),
            tax: todo!(),
            tax_exempt: todo!(),
            tax_id_data: todo!(),
            test_clock: todo!(),
        },
    );
}

#[derive(Debug, ToSql, FromSql)]
pub struct MyTimeStamp(pub NaiveDateTime);

impl Default for MyTimeStamp {
    fn default() -> Self {
        let x = NaiveDateTime::from_timestamp(0, 0);
        Self(x)
    }
}

#[derive(Debug, Default, ToSql, FromSql)]
pub struct MigrationId(pub Uuid);

entity! {
  pub struct Migration {
    pub id : Uuid,
    pub name: String,
    pub seq_order: i32,
    pub up: String,
    pub down: String,
    pub applied_on: NaiveDateTime,
  }
}

fn migration_table() -> String {
    "migrations".to_string()
}

pub fn default_migration() -> Migration {
    Migration {
        id: Uuid::from_str("1e270780-9a16-4949-8a37-1a37a11f1199").unwrap(),
        name: "".to_string(),
        seq_order: 0,
        up: "".to_string(),
        down: "".to_string(),
        applied_on: NaiveDateTime::from_timestamp(0, 0),
    }
}

fn map_migration_with_err(res: Result<Row, tokio_postgres::Error>) -> Migration {
    match res {
        Ok(r) => Migration::from_row(r),
        Err(_) => default_migration(),
    }
}

pub fn find_one<'a>(
    client: &'a Transaction,
) -> impl FnOnce(Vec<MigrationCriteria>) -> BoxFuture<'a, Result<Option<Migration>, anyhow::Error>>
{
    move |crit: Vec<MigrationCriteria>| {
        Box::pin(async move {
            let cond = crit.iter().map(|x| x.to_query_condition()).collect();
            select(client, &migration_table(), &cond, Migration::from_row).await
        })
    }
}

pub fn find_all<'a>(
    client: &'a Client,
) -> impl FnOnce() -> BoxFuture<'a, Result<Vec<Migration>, anyhow::Error>> {
    move || {
        Box::pin(async move {
            let cond: Vec<QueryCondition> = vec![];
            select_all(client, &migration_table(), &cond, Migration::from_row).await
        })
    }
}

pub fn create<'a>(
    client: &'a Transaction,
) -> impl FnOnce(Migration) -> BoxFuture<'a, Result<(), anyhow::Error>> {
    move |migration: Migration| {
        Box::pin(async move {
            let fields = field_names_without_id(Migration::field_names());
            insert(
                client,
                &migration_table(),
                &"id".to_string(),
                fields.as_slice(),
                &migration.id,
                &migration.to_params_x(),
            )
            .await
        })
    }
}

pub fn find_all_stream<'a>(
    client: &'a Client,
) -> impl FnOnce(Vec<MigrationCriteria>) -> BoxFuture<'a, Result<BoxStream<'a, Migration>, anyhow::Error>>
{
    move |crit| {
        Box::pin(async move {
            let conds: Vec<QueryCondition> = crit.iter().map(|c| c.to_query_condition()).collect();
            let r = select_raw(client, &migration_table(), &conds, map_migration_with_err).await;
            match r {
                Ok(m) => Ok(m.boxed()),
                Err(e) => Err(e),
            }
        })
    }
}

/*
pub fn find_all<'a>(client: &'a Client, table: &String) -> Result<BoxStream<'static, Migration>, anyhow::Error> {
  move || {
    Box::pin(async move {
      let r = select_raw(client, &migration_table(), &vec![], map_migration_with_err).await;
      Box::pin(r)
    })
    // Box::pin(async move {
    //   select_raw(client, &migration_table(), &vec![], Migration::from_row)
    // })
  }
}
*/
