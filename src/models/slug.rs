use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::pg::Pg;
use diesel::sql_types::Text;
use std::str::FromStr;

#[derive(Debug, Clone, Eq, PartialEq, FromSqlRow)]
pub struct Slug(String);
impl AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl FromSql<Text, Pg> for Slug {
    fn from_sql(
        bytes: Option<&<Pg as Backend>::RawValue>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as FromSql<Text, Pg>>::from_sql(bytes)?;
        Slug::from_str(&s).map_err(|_| format!("Bad slug {:?}", s).into())
    }
}
impl std::fmt::Display for Slug {
    fn fmt(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(out)
    }
}
impl std::ops::Deref for Slug {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl FromStr for Slug {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
            Ok(Slug(s.to_string()))
        } else {
            Err(())
        }
    }
}
