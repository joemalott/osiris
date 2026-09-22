//! Dense per-tile storage sized to the map.

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Grid<T> {
    width: i32,
    height: i32,
    data: Vec<T>,
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
        }
    }

    pub fn from_fn(width: i32, height: i32, mut f: impl FnMut(i32, i32) -> T) -> Self {
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                data.push(f(x, y));
            }
        }
        Self { width, height, data }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
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

    pub fn set(&mut self, x: i32, y: i32, v: T) {
        if let Some(i) = self.index(x, y) {
            self.data[i] = v;
        }
    }

    pub fn update(&mut self, x: i32, y: i32, f: impl FnOnce(T) -> T) {
        if let Some(i) = self.index(x, y) {
            self.data[i] = f(self.data[i]);
        }
    }

    pub fn fill(&mut self, v: T) {
        self.data.fill(v);
    }

    pub fn as_slice(&self) -> &[T] {
        &self.data
    }
}
