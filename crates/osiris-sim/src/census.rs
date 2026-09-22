//! The city's age census: how many citizens there are of each age 0..100. Newcomers
//! get ages drawn from the random pool; leavers are removed the same way. The
//! working-age share of the census sets the workforce.

use crate::rng::Rng;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Census {
    pub at_age: Vec<i32>,
}

impl Default for Census {
    fn default() -> Self {
        Self { at_age: vec![0; 100] }
    }
}

impl Census {
    pub fn total(&self) -> i32 {
        self.at_age.iter().sum()
    }

    pub fn add(&mut self, rng: &Rng, people: i32) {
        let mut odd = false;
        for i in 0..people.max(0) as usize {
            let mut age = rng.from_pool(i) & 0x3f;
            if age > 50 {
                age -= 30;
            } else if age < 10 && odd {
                age += 20;
            }
            self.at_age[age as usize] += 1;
            odd = !odd;
        }
    }

    pub fn remove(&mut self, rng: &Rng, mut people: i32) {
        let mut index = 0;
        let mut empty = 0;
        while people > 0 && empty < 100 {
            let age = (rng.from_pool(index) & 0x3f) as usize;
            index += 1;
            if self.at_age[age] <= 0 {
                empty += 1;
            } else {
                self.at_age[age] -= 1;
                people -= 1;
                empty = 0;
            }
        }
        // Random picks failed: take from age 10 upward, wrapping.
        let mut age = 10;
        empty = 0;
        while people > 0 && empty < 100 {
            if self.at_age[age] <= 0 {
                empty += 1;
            } else {
                self.at_age[age] -= 1;
                people -= 1;
                empty = 0;
            }
            age = (age + 1) % 100;
        }
    }

    /// People of working age (20..50).
    pub fn working_age(&self) -> i32 {
        self.at_age[20..50].iter().sum()
    }
}
