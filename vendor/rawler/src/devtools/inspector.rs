#[cfg(feature = "inspector")]
#[macro_export]
macro_rules! inspector {
      ($( $args:expr ),*) => { println!( $( $args ),* ); }
  }

#[cfg(not(feature = "inspector"))]
#[macro_export]
macro_rules! inspector {
  ($( $args:expr ),*) => {};
}
