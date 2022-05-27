use postgres_derive::FromSql;
use serde::Deserialize;
use uuid::Uuid;

use crate::postgres_common::core::{entity, QueryCondition};

#[derive(Debug, Clone, Copy, Deserialize, postgres_derive::ToSql, FromSql)]
pub struct InvitationId(Uuid);

entity! {
    pub struct Invitation {
        id: InvitationId,
        email: String,
    }
}
