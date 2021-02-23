// Adapted from https://github.com/bluss/maplit for HeaderMaps
macro_rules! headers {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(headers!(@single $rest)),*]));

    ($($key:expr => $value:expr,)+) => { headers!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let _cap = headers!(@count $($key),*);
            let mut _map = headers::HeaderMap::with_capacity(_cap);
            $(
                let _ = _map.insert($key, $value);
            )*
            _map
        }
    };
}
