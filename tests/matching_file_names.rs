#[cfg(test)]
#[macro_use]
extern crate quickcheck;


#[cfg(test)]
mod tests {
    quickcheck! {
        fn environment_test(xs: bool) -> bool {
            xs == !(!xs)
        }
    }
}