use crate::postgres_common::core::{entity, insert, select, update, QueryCondition};

use futures::{future::BoxFuture, TryFutureExt};
use postgres_derive::FromSql;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{self, Display},
    future::Future,
    hash::Hash,
};
use tokio_postgres::{Client, Row, Transaction};
use uuid::Uuid;
use validator::{Validate, ValidationError, ValidationErrors};

use super::common::field_names_without_id;

#[derive(Debug, Clone, Copy, Deserialize, postgres_derive::ToSql, FromSql, Default)]
pub struct UserId(Uuid);

entity! {
    #[derive(Debug, Default)]
    pub struct User {
        id: UserId,
        username: String,
        password: String,
        roles: String,
        account_id: Uuid,
    }
}

#[derive(Debug, Clone, Copy, Deserialize, postgres_derive::ToSql, FromSql, Default)]
pub struct AccountId(Uuid);

entity! {
    #[derive(Debug, Default)]
    pub struct Account {
        id: AccountId,
        name: String,
    }
}

pub fn map_from_result(result: Result<Row, tokio_postgres::Error>) -> User {
    match result {
        Ok(u) => User::from_row(u),
        Err(_) => User::default(),
    }
}

#[derive(Debug, Validate, Deserialize, Clone)]
pub struct UserDto {
    pub id: Uuid,
    #[validate(length(min = 3, message = "username_required"))]
    pub username: String,
    #[validate(length(min = 8, max = 18, message = "password_between_8_and_18_chars"))]
    pub password: String,
    #[validate(length(min = 1, message = "roles_required"))]
    pub roles: String,
    pub account_id: Uuid,
}

pub fn user_from_dto(dto: UserDto) -> User {
    User {
        id: UserId(dto.id),
        username: dto.username,
        password: dto.password,
        roles: dto.roles,
        account_id: dto.account_id,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UserValidationError {
    #[error("Username invalid: {0}")]
    UsernameInvalid(String),
    #[error("Password invalid: {0}")]
    PasswordInvalid(String),
    #[error("Roles invalid: {0}")]
    RolesInvalid(String),
}

#[derive(Debug, thiserror::Error)]
pub struct UserValidationErrors {
    username: Option<String>,
    password: Option<String>,
    roles: Option<String>,
}

fn hash_map_to_string(hash_map: HashMap<String, String>) -> String {
    hash_map.into_iter().fold("".to_string(), |acc, x| {
        let new_msg = format!("{}: {}", x.0, x.1);
        if acc.is_empty() {
            new_msg
        } else {
            format!("{}, {}", acc, new_msg)
        }
    })
}

impl Display for UserValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = vec![
            self.username.as_ref(),
            self.password.as_ref(),
            self.roles.as_ref(),
        ]
        .into_iter()
        .fold("".to_string(), |acc, x| {
            let msg = match x {
                Some(m) => m,
                None => "",
            }
            .to_owned();
            if acc.is_empty() {
                msg
            } else {
                format!("{}, {}", acc, msg)
            }
        });
        write!(f, "{}", msg)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateSuperUserError {
    #[error("User invalid")]
    UserInvalid(HashMap<String, String>),

    #[error("Only one super user can exist on a system")]
    SuperUserExists,

    #[error("Repo Error: {0}")]
    RepoError(String),

    #[error("An unknown error occurred")]
    UnknownError,

    #[error("Account invalid")]
    AccountInvalid(HashMap<String, String>),

    #[error("Account exits")]
    AccountExists,
}

impl From<anyhow::Error> for CreateSuperUserError {
    fn from(_: anyhow::Error) -> Self {
        CreateSuperUserError::UnknownError
    }
}

fn hash_map_from_validation_errors(e: ValidationErrors) -> HashMap<String, String> {
    let field_errors = e.field_errors();
    field_errors
        .into_iter()
        .map(|fe| {
            let (field, msgs) = fe;
            let msg = msgs.into_iter().fold("".to_string(), |acc, x| {
                if acc.is_empty() {
                    x.to_string()
                } else {
                    format!("{}, {}", acc, x.to_string())
                }
            });
            (field.to_string(), msg)
        })
        .collect()
}

pub fn concat_validation_errors(v_errs_opt: Option<&&Vec<ValidationError>>) -> String {
    match v_errs_opt {
        None => "".to_string(),
        Some(v_errs) => v_errs.into_iter().fold("".to_string(), |acc, x| {
            if acc.is_empty() {
                match &x.message {
                    Some(m) => m.to_string(),
                    None => "".to_string(),
                }
            } else {
                format!("{}, {}", acc, x)
            }
        }),
    }
}

pub const USER_TABLE: &'static str = "users";

pub fn user_table() -> String {
    "users".to_string()
}

pub fn find_super_user<'a>(
    client: &'a Transaction,
) -> impl FnOnce() -> BoxFuture<'a, Result<Option<User>, CreateSuperUserError>> {
    move || {
        Box::pin(async move {
            let rol_crit = UserCriteria::RolesLike("%super_admin%".to_string());
            let crit = vec![rol_crit.to_query_condition()];
            select(client, &user_table(), &crit, User::from_row)
                .await
                .map_err(|_| CreateSuperUserError::RepoError("".to_string()))
        })
    }
}

pub fn insert_user<'a>(
    client: &'a Transaction,
) -> impl FnOnce(User) -> BoxFuture<'a, Result<(), CreateSuperUserError>> {
    move |user: User| {
        Box::pin(async move {
            let fields = field_names_without_id(User::field_names());
            insert(
                client,
                &user_table(),
                &"id".to_string(),
                fields.as_slice(),
                &user.id,
                &user.to_params_x(),
            )
            .await
            .map_err(|_| CreateSuperUserError::RepoError("".to_string()))
        })
    }
}

pub fn update_user<'a, 'b>(
    client: &'a Client,
) -> impl FnOnce(&'a User) -> BoxFuture<'a, Result<(), CreateSuperUserError>> {
    move |user: &'a User| {
        Box::pin(async move {
            let fields = field_names_without_id(User::field_names());
            update(
                client,
                &user_table(),
                &"id".to_string(),
                fields.as_slice(),
                &user.id,
                &user.to_params_x(),
            )
            .await
            .map_err(|_| CreateSuperUserError::RepoError("".to_string()))
        })
    }
}

// todo: move this with the user dto
#[derive(Serialize, Deserialize, Validate, Clone)]
pub struct AccountDto {
    pub id: Uuid,
    pub name: String,
}

pub async fn create_super_user<FA, FB, FC, FD>(
    find_super_user: impl FnOnce() -> FA,
    insert: impl FnOnce(User) -> FB,
    insert_account: impl FnOnce(Account) -> FC,
    find_account_by_id: impl FnOnce(AccountId) -> FD,
    user_dto: &UserDto,
    account_dto: &AccountDto,
) -> Result<(), CreateSuperUserError>
where
    FA: Future<Output = Result<Option<User>, CreateSuperUserError>>,
    FB: Future<Output = Result<(), CreateSuperUserError>>,
    FC: Future<Output = Result<(), CreateAccountError>>,
    FD: Future<Output = Result<Option<Account>, CreateAccountError>>,
{
    let _ = user_dto.validate().map_err(|e| {
        let hash_map = hash_map_from_validation_errors(e);
        CreateSuperUserError::UserInvalid(hash_map)
    })?;
    let _ = account_dto.validate().map_err(|e| {
        let hash_map = hash_map_from_validation_errors(e);
        CreateSuperUserError::AccountInvalid(hash_map)
    })?;
    let user = user_from_dto(user_dto.clone());
    let maybe_existing_user = find_super_user().await?;
    match maybe_existing_user {
        Some(_) => Err(CreateSuperUserError::SuperUserExists),
        None => {
            let maybe_existing_account = find_account_by_id(AccountId(account_dto.id))
                .await
                .map_err(|e| CreateSuperUserError::RepoError(e.to_string()))?;
            match maybe_existing_account {
                Some(_) => Err(CreateSuperUserError::AccountExists),
                None => {
                    let account = Account {
                        id: AccountId(account_dto.id),
                        name: account_dto.clone().name,
                    };
                    let _ = insert_account(account)
                        .await
                        .map_err(|e| CreateSuperUserError::RepoError(e.to_string()))?;
                    let ins_res = insert(user).await;
                    match ins_res {
                        Ok(_) => Ok(()),
                        Err(e) => Err(e),
                    }
                }
            }
        }
    }
}

pub fn account_table() -> String {
    "accounts".to_string()
}

pub enum CreateAccountError {
    RepoError(String),
}

impl ToString for CreateAccountError {
    fn to_string(&self) -> String {
        match self {
            Self::RepoError(es) => es.to_owned(),
        }
    }
}

pub struct Blah<'a> {
    pub x: &'a dyn FnOnce(String) -> BoxFuture<'a, String>,
}

fn foo<'a>(x: String) -> BoxFuture<'a, String> {
    Box::pin(async move { format!("{}!!!", x) })
}

pub fn blah() {
    Blah { x: &foo };
}

pub fn insert_account<'a>(
    client: &'a Transaction,
) -> impl FnOnce(Account) -> BoxFuture<'a, Result<(), CreateAccountError>> {
    move |account: Account| {
        Box::pin(async move {
            let fields = field_names_without_id(Account::field_names());
            insert(
                client,
                &account_table(),
                &"id".to_string(),
                fields.as_slice(),
                &account.id,
                &account.to_params_x(),
            )
            .await
            // todo: put a real error here
            .map_err(|_| CreateAccountError::RepoError("".to_string()))
        })
    }
}

pub fn find_account_by_id<'a>(
    client: &'a Transaction,
) -> impl FnOnce(AccountId) -> BoxFuture<'a, Result<Option<Account>, CreateAccountError>> {
    move |account_id: AccountId| {
        Box::pin(async move {
            let id_crit = AccountCriteria::IdEq(account_id);
            let cond = vec![id_crit.to_query_condition()];
            select(client, &account_table(), &cond, Account::from_row)
                .await
                .map_err(|e| CreateAccountError::RepoError(e.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use futures::{executor::block_on, future::BoxFuture};
    use uuid::Uuid;

    use crate::models::users::hash_map_to_string;

    use super::{
        create_super_user, Account, AccountDto, AccountId, CreateAccountError,
        CreateSuperUserError, User, UserDto,
    };

    fn user_dto() -> UserDto {
        UserDto {
            id: Uuid::from_str("9acd36f9-b9f4-4fd1-840c-c161a9fd3c41").unwrap(),
            username: "someusername".to_string(),
            password: "!Q2w3e4r5t".to_string(),
            roles: "super_user".to_string(),
            account_id: Uuid::from_str("a304f299-b547-4d3d-bd42-732f617b258a").unwrap(),
        }
    }

    fn account_dto() -> AccountDto {
        AccountDto {
            id: Uuid::from_str("3c3f5220-8b3d-40a3-8da2-196a69beaca8").unwrap(),
            name: "edb".to_string(),
        }
    }

    fn find_existing_super_user<'a>(
        count: &'a mut u8,
    ) -> impl FnOnce() -> BoxFuture<'a, Result<Option<User>, CreateSuperUserError>> {
        move || {
            *count += 1;
            Box::pin(async move { Ok(None) })
        }
    }

    fn insert_user_mock<'a>(
        count: &'a mut u8,
    ) -> impl FnOnce(User) -> BoxFuture<'a, Result<(), CreateSuperUserError>> {
        move |_: User| {
            *count += 1;
            Box::pin(async move { Ok(()) })
        }
    }

    fn find_account_by_id<'a>(
        count: &'a mut u8,
    ) -> impl FnOnce(AccountId) -> BoxFuture<'a, Result<Option<Account>, CreateAccountError>> {
        move |_: AccountId| {
            *count += 1;
            Box::pin(async move { Ok(None) })
        }
    }

    fn fake_account() -> Account {
        Account {
            id: AccountId(Uuid::from_str("ac41d7b5-248c-415c-8728-9cb3bd91a6fb").unwrap()),
            name: "fake".to_string(),
        }
    }

    fn find_account_by_id_mock_found<'a>(
        count: &'a mut u8,
    ) -> impl FnOnce(AccountId) -> BoxFuture<'a, Result<Option<Account>, CreateAccountError>> {
        move |_: AccountId| {
            *count += 1;
            Box::pin(async move { Ok(Some(fake_account())) })
        }
    }

    fn insert_account_mock<'a>(
        count: &'a mut u8,
    ) -> impl FnOnce(Account) -> BoxFuture<'a, Result<(), CreateAccountError>> {
        move |_: Account| {
            *count += 1;
            Box::pin(async move { Ok(()) })
        }
    }

    #[test]
    pub fn test_create_ok() {
        let mut find_existing_super_user_count: u8 = 0;
        let mut insert_count: u8 = 0;
        let mut find_account_by_id_count: u8 = 0;
        let mut insert_account_count: u8 = 0;

        let res = block_on(create_super_user(
            find_existing_super_user(&mut find_existing_super_user_count),
            insert_user_mock(&mut insert_count),
            insert_account_mock(&mut insert_account_count),
            find_account_by_id(&mut find_account_by_id_count),
            &user_dto(),
            &account_dto(),
        ));
        match res {
            Ok(_) => {
                assert_eq!(1, find_existing_super_user_count);
                assert_eq!(1, insert_count);
                assert_eq!(1, find_account_by_id_count);
                assert_eq!(1, insert_account_count);
            }
            Err(_e) => assert!(false, "Super user creation failed"),
        }
    }

    #[test]
    pub fn test_create_super_user_fails_with_invalid_user() {
        let dto = UserDto {
            username: "".to_string(),
            password: "".to_string(),
            ..user_dto()
        };
        let mut find_su_count: u8 = 0;
        let mut insert_count: u8 = 0;
        let mut insert_account_count: u8 = 0;
        let mut find_account_by_id_count: u8 = 0;
        let res = block_on(create_super_user(
            find_existing_super_user(&mut find_su_count),
            insert_user_mock(&mut insert_count),
            insert_account_mock(&mut insert_account_count),
            find_account_by_id(&mut find_account_by_id_count),
            &dto,
            &account_dto(),
        ));
        match res {
            Ok(_) => assert!(false, "Ok encountered where Err expected"),
            Err(e) => match e {
                CreateSuperUserError::UserInvalid(map) => {
                    println!("{}", hash_map_to_string(map));
                    assert_eq!(0, find_su_count);
                    assert_eq!(0, insert_count);
                    assert_eq!(0, find_account_by_id_count);
                    assert_eq!(0, insert_account_count);
                }
                _ => assert!(false, "Incorrect variant found"),
            },
        }
    }

    #[test]
    pub fn test_create_super_user_fails_with_account_found() {
        let mut find_su_count: u8 = 0;
        let mut insert_count: u8 = 0;
        let mut insert_account_count: u8 = 0;
        let mut find_account_by_id_count: u8 = 0;
        let res = block_on(create_super_user(
            find_existing_super_user(&mut find_su_count),
            insert_user_mock(&mut insert_count),
            insert_account_mock(&mut insert_account_count),
            find_account_by_id_mock_found(&mut find_account_by_id_count),
            &user_dto(),
            &account_dto(),
        ));
        match res {
            Ok(_) => assert!(false, "Ok encountered where Err expected"),
            Err(e) => match e {
                CreateSuperUserError::AccountExists => {
                    assert!(true, "Account Exists error encountered")
                }
                _ => assert!(false, "Incorrect variant found"),
            },
        }
    }

    #[test]
    pub fn test_create_super_user_fails_with_account_insert_error() {
        let mut find_su_count: u8 = 0;
        let mut insert_count: u8 = 0;
        let mut insert_account_count: u8 = 0;
        let mut find_account_by_id_count: u8 = 0;

        let insert_account_error = |_: Account| {
            insert_account_count += 1;
            async { Err(CreateAccountError::RepoError("Repo Error".to_string())) }
        };
        let res = block_on(create_super_user(
            find_existing_super_user(&mut find_su_count),
            insert_user_mock(&mut insert_count),
            insert_account_error,
            find_account_by_id(&mut find_account_by_id_count),
            &user_dto(),
            &account_dto(),
        ));
        match res {
            Ok(_) => assert!(false, "Ok encountered where Err expected"),
            Err(e) => match e {
                CreateSuperUserError::RepoError(e) => assert_eq!("Repo Error".to_string(), e),
                _ => assert!(false, "Incorrect variant found"),
            },
        }
    }
}
