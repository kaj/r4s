use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types::Timestamptz;
use fluent::types::FluentType;
use fluent::FluentValue;
use intl_memoizer::concurrent::IntlLangMemoizer as CcIntlLangMemoizer;
use intl_memoizer::IntlLangMemoizer;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd)]
pub struct DateTime(chrono::DateTime<chrono::Utc>);

impl DateTime {
    pub fn wrap(v: chrono::DateTime<chrono::Utc>) -> Self {
        DateTime(v)
    }
    pub fn raw(&self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

impl Queryable<Timestamptz, Pg> for DateTime {
    type Row =
        <chrono::DateTime<chrono::Utc> as Queryable<Timestamptz, Pg>>::Row;
    fn build(row: Self::Row) -> Self {
        DateTime(chrono::DateTime::<chrono::Utc>::build(row))
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
        self.0.format("%Y-%m-%d %H:%M").to_string().into()
    }
    fn as_string_threadsafe(
        &self,
        _intls: &CcIntlLangMemoizer,
    ) -> Cow<'static, str> {
        self.0.format("%Y-%m-%d %H:%M").to_string().into()
    }
}
