use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::pg::Pg;
use diesel::sql_types::Text;
use std::fmt::{self, Display};
use std::str::FromStr;

/// Either "sv" or "en".
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, FromSqlRow)]
pub enum MyLang {
    #[default]
    En,
    Sv,
}

impl FromStr for MyLang {
    type Err = BadLang;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "en" => Ok(MyLang::En),
            "sv" => Ok(MyLang::Sv),
            _ => Err(BadLang(value.into())),
        }
    }
}

#[derive(Debug)]
pub struct BadLang(String);
impl std::error::Error for BadLang {}
impl fmt::Display for BadLang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bad language {:?}", self.0)
    }
}

impl AsRef<str> for MyLang {
    fn as_ref(&self) -> &str {
        match self {
            MyLang::En => "en",
            MyLang::Sv => "sv",
        }
    }
}
impl Display for MyLang {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        self.as_ref().fmt(out)
    }
}

impl FromSql<Text, Pg> for MyLang {
    fn from_sql(
        bytes: <Pg as Backend>::RawValue<'_>,
    ) -> diesel::deserialize::Result<Self> {
        Ok(<String as FromSql<Text, Pg>>::from_sql(bytes)?.parse()?)
    }
}
