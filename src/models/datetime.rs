use chrono::{Datelike, Utc};
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types::Timestamptz;
use fluent::types::FluentType;
use fluent::FluentValue;
use intl_memoizer::concurrent::IntlLangMemoizer as CcIntlLangMemoizer;
use intl_memoizer::IntlLangMemoizer;
use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt::Display;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd)]
pub struct DateTime(chrono::DateTime<chrono::Utc>);

impl DateTime {
    pub fn wrap(v: chrono::DateTime<chrono::Utc>) -> Self {
        DateTime(v)
    }
    pub fn raw(&self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }

    pub(crate) fn year(&self) -> i16 {
        self.0.year() as i16
    }

    /// Returns the age since a post was updated in years if the post
    /// is considere old, or None if the post is considered not so
    /// old.
    pub fn old_age(&self) -> Option<i64> {
        let age = Utc::now() - self.raw();
        let age = age.num_days() * 1000 / 365_240;
        if age >= 10 {
            Some(age)
        } else {
            None
        }
    }
}

impl Display for DateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.format("%Y-%m-%d %H:%M").fmt(f)
    }
}

impl FromStr for DateTime {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::wrap(s.parse()?))
    }
}

impl Queryable<Timestamptz, Pg> for DateTime {
    type Row =
        <chrono::DateTime<chrono::Utc> as Queryable<Timestamptz, Pg>>::Row;
    fn build(
        row: Self::Row,
    ) -> Result<DateTime, Box<dyn StdError + Send + Sync + 'static>> {
        Ok(DateTime(chrono::DateTime::<chrono::Utc>::build(row)?))
    }
}

impl<'a> From<&'a DateTime> for FluentValue<'static> {
    fn from(val: &'a DateTime) -> FluentValue<'static> {
        FluentValue::Custom(val.duplicate())
    }
}

impl FluentType for DateTime {
    fn duplicate(&self) -> Box<dyn FluentType + Send + 'static> {
        Box::new(*self)
    }
    fn as_string(&self, _intls: &IntlLangMemoizer) -> Cow<'static, str> {
        self.to_string().into()
    }
    fn as_string_threadsafe(
        &self,
        _intls: &CcIntlLangMemoizer,
    ) -> Cow<'static, str> {
        self.to_string().into()
    }
}
