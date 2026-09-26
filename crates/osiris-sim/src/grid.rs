//! Dense per-tile storage sized to the map.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Grid<T> {
    width: i32,
    height: i32,
    data: Vec<T>,
    /// Names the contents with a number no other contents have had (a clone keeps it
    /// until one of the two changes), so searches can remember what they found for
    /// them. 0 until asked for, and again after every change.
    #[serde(skip)]
    version: AtomicU64,
}

impl<T: Clone> Clone for Grid<T> {
    fn clone(&self) -> Self {
        Self { width: self.width, height: self.height, data: self.data.clone(), version: AtomicU64::new(self.version.load(Ordering::Relaxed)) }
    }
}

static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

pub(crate) fn fresh_version() -> u64 {
    NEXT_VERSION.fetch_add(1, Ordering::Relaxed)
}

impl<T: PartialEq> PartialEq for Grid<T> {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height && self.data == other.data
    }
}

impl<T: Copy + Default> Grid<T> {
    pub fn new(width: i32, height: i32) -> Self {
        Self::filled(width, height, T::default())
    }
}

impl<T: Copy> Grid<T> {
    pub fn filled(width: i32, height: i32, value: T) -> Self {
        Self {
            width,
            height,
            data: vec![value; (width.max(0) * height.max(0)) as usize],
            version: AtomicU64::new(0),
        }
    }

    pub fn from_fn(width: i32, height: i32, mut f: impl FnMut(i32, i32) -> T) -> Self {
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                data.push(f(x, y));
            }
        }
        Self { width, height, data, version: AtomicU64::new(0) }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    /// Identifies the current contents: equal versions mean equal contents.
    pub fn version(&self) -> u64 {
        match self.version.load(Ordering::Relaxed) {
            0 => {
                let v = fresh_version();
                self.version.store(v, Ordering::Relaxed);
                v
            }
            v => v,
        }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        self.contains(x, y).then(|| (y * self.width + x) as usize)
    }

    pub fn get(&self, x: i32, y: i32) -> Option<T> {
        self.index(x, y).map(|i| self.data[i])
    }

    /// Value at `(x, y)`, or `outside` beyond the edges.
    pub fn at_or(&self, x: i32, y: i32, outside: T) -> T {
        self.get(x, y).unwrap_or(outside)
    }

    pub fn fill(&mut self, v: T) {
        self.data.fill(v);
        *self.version.get_mut() = 0;
    }

    pub fn as_slice(&self) -> &[T] {
        &self.data
    }
}

impl<T: Copy + PartialEq> Grid<T> {
    pub fn set(&mut self, x: i32, y: i32, v: T) {
        if let Some(i) = self.index(x, y)
            && self.data[i] != v
        {
            self.data[i] = v;
            *self.version.get_mut() = 0;
        }
    }

    pub fn update(&mut self, x: i32, y: i32, f: impl FnOnce(T) -> T) {
        if let Some(i) = self.index(x, y) {
            let v = f(self.data[i]);
            if self.data[i] != v {
                self.data[i] = v;
                *self.version.get_mut() = 0;
            }
        }
    }
}
