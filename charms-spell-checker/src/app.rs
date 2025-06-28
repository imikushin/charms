use charms_data::util;
use serde::Serialize;
use sp1_primitives::io::SP1PublicValues;

pub(crate) fn to_public_values<T: Serialize>(t: &T) -> SP1PublicValues {
    SP1PublicValues::from(
        util::write(t)
            .expect("(app, tx, x) should serialize successfully")
            .as_slice(),
    )
}

#[cfg(test)]
mod test {
    #[test]
    fn dummy() {}
}
