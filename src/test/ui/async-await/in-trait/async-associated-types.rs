// edition: 2021

#![feature(async_fn_in_trait)]
#![allow(incomplete_features)]

use std::fmt::Debug;

trait MyTrait<'a, 'b, T> where Self: 'a, T: Debug + Sized + 'b {
    type MyAssoc;// = (&'a T, &'b U);

    async fn foo(&'a self, key: &'b T) -> Self::MyAssoc;
}

impl<'a, 'b, T: Debug + Sized + 'b, U: 'a> MyTrait<'a, 'b, T> for U {
    type MyAssoc = (&'a U, &'b T);

    async fn foo(&'a self, key: &'b T) -> (&'a U, &'b T) {
        (self, key)
    }
}
//~^^^^ ERROR cannot infer an appropriate lifetime for lifetime parameter `'a` due to conflicting requirements
//~| ERROR cannot infer an appropriate lifetime for lifetime parameter `'b` due to conflicting requirements

fn main() {}
