use cli_fallback::Fallback;

#[test]
fn no_attributes() {
    #[derive(Fallback)]
    struct X {
        field: u8,
    }

    let x = X { field: 42 };
    let _ = x.field;
}

#[test]
fn happy_path() {
    const ZERO: u8 = 0;

    #[derive(Fallback)]
    struct X {
        #[fallback(false)]
        /// Something here and there.
        /// fallback: false
        field: Option<bool>,

        #[fallback(ZERO)]
        /// Something here and there.
        /// fallback: ZERO
        number: Option<u8>,
    }

    let x = X {
        field: Some(true),
        number: Some(7),
    };

    assert_eq!(x.field(), true);
    assert_eq!(x.number(), 7);

    let x = X {
        field: None,
        number: None,
    };

    assert_eq!(x.field(), false);
    assert_eq!(x.number(), ZERO);
}

// #[comment]
// /// This should be commented
// pub struct Commented {
//     field: Option<bool>,
// }
