pub struct Guard<F: FnOnce()> {
    f: Option<F>,
}
impl<F: FnOnce()> Guard<F> {
    pub fn new(f: F) -> Self {
        Self { f: Some(f) }
    }
    pub fn cancel(&mut self) {
        self.f = None;
    }
}
impl<F: FnOnce()> Drop for Guard<F> {
    fn drop(&mut self) {
        if let Some(f) = self.f.take() {
            f();
        }
    }
}
