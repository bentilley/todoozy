use ignore::{Walk, WalkBuilder};

pub fn get_files(to_exclude: &[String]) -> Walk {
    let mut exclude = ignore::overrides::OverrideBuilder::new("./");
    exclude.add("!.git/").unwrap();

    for e in to_exclude {
        exclude.add(&format!("!{}", e)).unwrap();
    }

    WalkBuilder::new("./")
        .hidden(false)
        .overrides(exclude.build().unwrap())
        .build()
}
