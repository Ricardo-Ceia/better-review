pub fn greeting(name: &str) -> String {
    format!("Hello, {name}.")
}

pub fn headline() -> String {
    "Review queue ready.".to_string()
}

pub fn summary(items: &[&str]) -> String {
    items.join(", ")
}

pub fn footer() -> &'static str {
    "Press c to commit."
}
