use barrel::{types, Migration};

use crate::Sql;

pub fn migration() -> String {
    let mut m = Migration::new();

    m.create_table("persons", |t| {
        t.add_column("id", types::primary());
        t.add_column("name", types::varchar(255));
        t.add_column("city", types::varchar(255));
    });

    m.make::<Sql>()
}
