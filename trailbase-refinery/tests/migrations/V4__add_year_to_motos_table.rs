use barrel::{types, Migration};

use crate::Sql;

pub fn migration() -> String {
    let mut m = Migration::new();

    m.change_table("motos", |t| {
        t.add_column("brand", types::varchar(255).nullable(true));
    });

    m.make::<Sql>()
}
