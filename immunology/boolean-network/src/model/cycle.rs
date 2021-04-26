use std::ops::Range;

pub struct CycleFinder<T> {
    power: usize,
    lambda: usize,
    mu: Option<usize>,
    tortoise: Option<T>,
}

impl<T> CycleFinder<T> where T: PartialEq {
    pub fn new() -> Self {
        Self {
            power: 1,
            lambda: 0,
            mu: Default::default(),
            tortoise: Default::default(),
        }
    }

    // A (sequential) implementation of Brent's algorithm to find a cycle in a collection.
    pub fn check_next<'a, I>(&mut self, collection: &'a I, next: T) -> Option<Range<usize>>
    where
        I: IntoIterator<Item = &'a T> + Copy,
        T: 'a,
    {
        if self.tortoise.is_none() {
            self.tortoise = Some(next);
            return None;
        }

        let tortoise = self.tortoise.as_mut().unwrap();
        let hare = &next;

        self.lambda += 1;

        if tortoise != hare {
            if self.power == self.lambda {
                *tortoise = next;
                self.power *= 2;
                self.lambda = 0;
            }

            return None;
        }

        let mu = collection
            .into_iter()
            .zip(collection.into_iter().skip(self.lambda))
            .take_while(|(tortoise, hare)| tortoise != hare)
            .count();

        self.mu.insert(mu);

        Some(mu..(mu + self.lambda))
    }
}
