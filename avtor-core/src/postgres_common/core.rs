use anyhow::Error;
extern crate core;
extern crate proc_macro;
use futures::{
    future::BoxFuture, stream::Iter, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt,
};
use tokio_postgres::{types::ToSql, Client, Row, RowStream, Statement, Transaction};

trait MyTransaction<'a> {
    fn prepare(query: &str) -> BoxFuture<'a, Result<Statement, Error>>;
}

pub fn create_insert_sql(table: &String, id_field: &String, fields: &[String]) -> String {
    let fields_sql: String = fields
        .iter()
        .fold(id_field.clone(), |acc, x| format!("{}, {}", acc, x));
    let field_range = 2..fields.len() + 2;
    let field_params = field_range.fold("$1".to_string(), |acc, x| format!("{}, ${}", acc, x));
    format!(
        "insert into {} ({}) values ({})",
        table, fields_sql, field_params
    )
}

pub fn create_update_sql(table: &String, id_field: &String, fields: &[String]) -> String {
    let (head, tail) = fields.split_at(1);
    let first = format!("{} = $1", head.first().unwrap());
    let (fields_sql, _) = tail.into_iter().fold((first, 2), |acc, x| {
        let (q, i) = acc;
        (format!("{} , {} = ${}", q, x, i.to_string()), i + 1)
    });
    format!(
        "update {} set {} where {} = ${}",
        table,
        fields_sql,
        id_field,
        fields.len() + 1
    )
}

pub async fn insert<'a>(
    client: &Transaction<'a>,
    table: &String,
    id_field: &String,
    fields: &[String],
    id_param: &(dyn ToSql + Sync),
    params: &[&(dyn ToSql + Sync)],
) -> Result<(), Error> {
    let insert_sql = create_insert_sql(table, id_field, fields);
    let stmt = client.prepare(&insert_sql).await?;
    let all_params = &[&[id_param], params].concat();
    client.execute(&stmt, all_params.as_slice()).await?;
    Ok(())
}

pub async fn update(
    client: &Client,
    table: &String,
    id_field: &String,
    fields: &[String],
    id_param: &(dyn ToSql + Sync),
    params: &[&(dyn ToSql + Sync)],
) -> Result<(), Error> {
    let update_sql = create_update_sql(table, id_field, fields);
    let stmt = client.prepare(&update_sql).await?;
    let all_params = &[params, &[id_param]].concat();
    client.execute(&stmt, all_params.as_slice()).await?;
    Ok(())
}

pub type Field = String;
pub type Value = (dyn ToSql + Sync);

pub enum QueryCondition<'a> {
    Eq(Field, &'a Value),
    Neq(Field, &'a Value),
    Gt(Field, &'a Value),
    Gte(Field, &'a Value),
    Lt(Field, &'a Value),
    Lte(Field, &'a Value),
    In(Field, &'a Value),
    Nin(Field, &'a Value),
    Like(Field, &'a Value),
    NLike(Field, &'a Value),
}

pub fn query_cond_to_string(q_cond: &QueryCondition, n: i32) -> String {
    match q_cond {
        QueryCondition::Eq(f, _) => format!("{} = ${}", f, n.to_string()),
        QueryCondition::Neq(f, _) => format!("{} != ${}", f, n.to_string()),
        QueryCondition::Gt(f, _) => format!("{} > ${}", f, n.to_string()),
        QueryCondition::Gte(f, _) => format!("{} >= ${}", f, n.to_string()),
        QueryCondition::Lt(f, _) => format!("{} <= ${}", f, n.to_string()),
        QueryCondition::Lte(f, _) => format!("{} <= ${}", f, n.to_string()),
        QueryCondition::In(f, _) => format!("{} = Any(${})", f, n.to_string()),
        QueryCondition::Nin(f, _) => format!("{} != Any(${})", f, n.to_string()),
        QueryCondition::Like(f, _) => format!("{} like ${}", f, n.to_string()),
        QueryCondition::NLike(f, _) => format!("{} not like ${}", f, n.to_string()),
    }
}

trait NewTrait: ToSql + Sized + Sync {}

pub fn generate_select<'a>(
    table: &String,
    query_conditions: &'a Vec<QueryCondition<'a>>,
) -> (String, Vec<&'a (dyn ToSql + Sync)>) {
    let base_query = format!("select * from {}", table);
    if query_conditions.is_empty() {
        (base_query, vec![])
    } else {
        let (where_part, _) = query_conditions
            .into_iter()
            .fold(("".to_string(), 1), |acc, x| {
                let (q, i) = acc;
                (format!("{} and {}", q, query_cond_to_string(x, i)), i + 1)
            });
        let query_with_where = format!("{} where 1 = 1 {}", base_query, where_part);
        let params = query_conditions
            .into_iter()
            .map(|x| match x {
                QueryCondition::Eq(_, p) => *p,
                QueryCondition::Neq(_, p) => *p,
                QueryCondition::Gt(_, p) => *p,
                QueryCondition::Gte(_, p) => *p,
                QueryCondition::Lt(_, p) => *p,
                QueryCondition::Lte(_, p) => *p,
                QueryCondition::In(_, p) => *p,
                QueryCondition::Nin(_, p) => *p,
                QueryCondition::Like(_, p) => *p,
                QueryCondition::NLike(_, p) => *p,
            })
            .collect();
        (query_with_where, params)
    }
}

pub async fn select_all<'a, F: Fn(Row) -> A + Send + 'static, A>(
    client: &Client,
    table: &String,
    query_conditions: &Vec<QueryCondition<'a>>,
    map_row: F,
) -> Result<Vec<A>, Error> {
    let (query, params) = generate_select(table, query_conditions);
    let stmt = client.prepare(&query).await?;
    let rows = client.query(&stmt, params.as_slice()).await?;
    Ok(rows.into_iter().map(map_row).collect())
}

pub async fn select_raw<'a, F: Fn(Result<Row, tokio_postgres::Error>) -> A + Send + 'static, A>(
    client: &Client,
    table: &String,
    query_conditions: &Vec<QueryCondition<'a>>,
    map_row: F,
) -> Result<impl Stream<Item = A>, Error> {
    let (query, params) = generate_select(table, query_conditions);
    let stmt = client.prepare(&query).await?;
    let rows = client.query_raw(&stmt, params.into_iter()).await?;
    Ok(rows.map(map_row))
}

/*
pub async fn select_all_stream<'a, A, F: Fn(RowStream) -> A + Send + 'static>(
    client: &Client,
    table: &String,
    query_conditions: &Vec<QueryCondition<'a>>,
    map_row: F,
) -> Result<impl Stream<Item = Result<A, Error>>> {
    let (query, params) = generate_select(table, query_conditions);
    let stmt = client.prepare(&query).await?;
    client.query_raw(&stmt, params.into_iter()).map_ok(map_row).into_stream()
    let rows = client.query_raw(&stmt, params.into_iter()).await?;
    rows.map_ok(map_row)
}
*/

pub async fn select<'a, F: Fn(Row) -> A + Send + 'static, A>(
    client: &Transaction<'a>,
    table: &String,
    query_conditions: &'a Vec<QueryCondition<'a>>,
    from_row: F,
) -> Result<Option<A>, Error> {
    let (query, params) = generate_select(table, query_conditions);
    let stmt = client.prepare(&query).await?;
    let row_opt = client.query_opt(&stmt, params.as_slice()).await?;
    Ok(row_opt.map(from_row))
}

macro_rules! entity {
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field_name:ident : $field_type:ty
            ),*$(,)+
    }) => {

        $(#[$struct_meta])*
        pub struct $name {
            $(
                $(#[$field_meta])*
                pub $field_name : $field_type,
            )*
        }

        paste::paste! {
            #[derive(Debug)]
            enum [<$name Fields>] {
                $([<$field_name:camel>]),*
            }

            impl std::fmt::Display for [<$name Fields>] {
                fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    std::fmt::Debug::fmt(self, f)
                }
            }
        }

        paste::paste! {
            #[derive(Debug)]
            pub enum [<$name Criteria>] {
                $([<$field_name:camel Eq>]($field_type)),*,
                $([<$field_name:camel Neq >]($field_type)),*,
                $([<$field_name:camel Gt>]($field_type)),*,
                $([<$field_name:camel Gte>]($field_type)),*,
                $([<$field_name:camel Lt>]($field_type)),*,
                $([<$field_name:camel Lte>]($field_type)),*,
                $([<$field_name:camel In>](Vec<$field_type>)),*,
                $([<$field_name:camel Nin>](Vec<$field_type>)),*,
                $([<$field_name:camel Like>]($field_type)),*,
                $([<$field_name:camel NLike>]($field_type)),*,
            }

            #[derive(Default,Debug)]
            pub struct [<$name CriteriaStruct>] {
                pub $([<$field_name _eq>]: Option<$field_type>),*,
                pub $([<$field_name _neq >]: Option<$field_type>),*,
                pub $([<$field_name _gt>]: Option<$field_type>),*,
                pub $([<$field_name _gte>]: Option<$field_type>),*,
                pub $([<$field_name _lt>]: Option<$field_type>),*,
                pub $([<$field_name _lte>]: Option<$field_type>),*,
                pub $([<$field_name _in>]: Vec<$field_type>),*,
                pub $([<$field_name _nin>]: Vec<$field_type>),*,
                pub $([<$field_name _like>]: Option<$field_type>),*,
                pub $([<$field_name _nlike>]: Option<$field_type>),*,
            }

            impl [<$name Criteria>] {
                fn to_query_condition<'a>(&'a self) -> QueryCondition<'a> {
                    match self {
                        $([<$name Criteria>]::[<$field_name:camel Eq>](x) => QueryCondition::Eq(stringify!($field_name).to_string(), x)),*,
                        $([<$name Criteria>]::[<$field_name:camel Neq>](x) => QueryCondition::Neq(stringify!($field_name).to_string(), x)),*,
                        $([<$name Criteria>]::[<$field_name:camel Gt>](x) => QueryCondition::Gt(stringify!($field_name).to_string(), x)),*,
                        $([<$name Criteria>]::[<$field_name:camel Gte>](x) => QueryCondition::Gte(stringify!($field_name).to_string(), x)),*,
                        $([<$name Criteria>]::[<$field_name:camel Lt>](x) => QueryCondition::Lt(stringify!($field_name).to_string(), x)),*,
                        $([<$name Criteria>]::[<$field_name:camel Lte>](x) => QueryCondition::Lte(stringify!($field_name).to_string(), x)),*,
                        $([<$name Criteria>]::[<$field_name:camel In>](x) => QueryCondition::In(stringify!($field_name).to_string(), x)),*,
                        $([<$name Criteria>]::[<$field_name:camel Nin>](x) => QueryCondition::Nin(stringify!($field_name).to_string(), x)),*,
                        $([<$name Criteria>]::[<$field_name:camel Like>](x) => QueryCondition::Like(stringify!($field_name).to_string(), x)),*,
                        $([<$name Criteria>]::[<$field_name:camel NLike>](x) => QueryCondition::NLike(stringify!($field_name).to_string(), x)),*,
                    }
                }
            }

            impl [<$name CriteriaStruct>] {
                fn to_criteria(self) -> Vec<[<$name Criteria>]> {
                    let mut c = vec![];
                    $(if let Some(x) = self.[<$field_name _eq>] {
                        c.push([<$name Criteria>]::[<$field_name:camel Eq>](x));
                    })*
                    $(if let Some(x) = self.[<$field_name _neq>] {
                        c.push([<$name Criteria>]::[<$field_name:camel Neq>](x));
                    })*
                    $(if let Some(x) = self.[<$field_name _gt>] {
                        c.push([<$name Criteria>]::[<$field_name:camel Gt>](x));
                    })*
                    $(if let Some(x) = self.[<$field_name _gte>] {
                        c.push([<$name Criteria>]::[<$field_name:camel Gte>](x));
                    })*
                    $(if let Some(x) = self.[<$field_name _lt>] {
                        c.push([<$name Criteria>]::[<$field_name:camel Lt>](x));
                    })*
                    $(if let Some(x) = self.[<$field_name _lte>] {
                        c.push([<$name Criteria>]::[<$field_name:camel Lte>](x));
                    })*
                    $(if !self.[<$field_name _in>].is_empty() {
                        c.push([<$name Criteria>]::[<$field_name:camel In>](self.[<$field_name _in>]));
                    })*
                    $(if !self.[<$field_name _nin>].is_empty() {
                        c.push([<$name Criteria>]::[<$field_name:camel Nin>](self.[<$field_name _nin>]));
                    })*
                    $(if let Some(x) = self.[<$field_name _like>] {
                        c.push([<$name Criteria>]::[<$field_name:camel Like>](x));
                    })*
                    $(if let Some(x) = self.[<$field_name _nlike>] {
                        c.push([<$name Criteria>]::[<$field_name:camel NLike>](x));
                    })*
                    c
                }
            }
        }


        impl $name {

            fn field_names() -> &'static [&'static str] {
                static NAMES: &'static [&'static str] = &[$(stringify!($field_name)),*];
                NAMES
            }

            fn field_types() -> &'static [&'static str] {
                static TYPES: &'static [&'static str] = &[$(stringify!($field_type)),*];
                TYPES
            }

            fn from_row(row: tokio_postgres::Row) -> $name {
                $(let $field_name: $field_type = row.get(stringify!($field_name));)*
                $name {
                    $($field_name),*
                }
           }

            fn to_params_x<'a>(&'a self) -> Vec<&'a (dyn tokio_postgres::types::ToSql + Sync)> {
                vec![
                    $(&self.$field_name as &(dyn tokio_postgres::types::ToSql + Sync)),*
                ][1..].into_iter().map(|x| *x as &(dyn tokio_postgres::types::ToSql + Sync)).collect::<Vec<&'a (dyn tokio_postgres::types::ToSql + Sync)>>()
            }
        }
    }
}

pub(crate) use entity;
