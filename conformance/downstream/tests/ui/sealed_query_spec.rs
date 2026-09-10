use ecs_under_test::query::QuerySpec;

struct UnsupportedQuery;

impl QuerySpec for UnsupportedQuery {}

fn main() {}
