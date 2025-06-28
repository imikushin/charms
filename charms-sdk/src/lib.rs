pub use charms_data as data;

#[macro_export]
macro_rules! main {
    ($path:path) => {
        fn main() {
            use charms_sdk::data::{is_simple_transfer, util, App, Data, Transaction};

            #[inline(always)]
            fn read_input() -> (App, Transaction, Data, Data) {
                util::read(std::io::stdin())
                    .expect("should deserialize (app, tx, x, w): (App, Transaction, Data, Data)")
            }

            let (app, tx, x, w): (App, Transaction, Data, Data) = read_input();
            assert!(is_simple_transfer(&app, &tx) || $path(&app, &tx, &x, &w));
        }
    };
}
