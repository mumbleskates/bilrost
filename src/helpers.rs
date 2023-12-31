pub(crate) struct FallibleIter<T, E> {
    iter: T,
    outcome: Result<(), E>,
}

impl<T, R, E> FallibleIter<T, E>
where
    T: Iterator<Item = Result<R, E>>,
{
    pub(crate) fn new(iter: T) -> Self {
        Self {
            iter,
            outcome: Ok(()),
        }
    }

    pub(crate) fn check(self) -> Result<(), E> {
        self.outcome
    }
}

impl<T, R, E> Iterator for &mut FallibleIter<T, E>
where
    T: Iterator<Item = Result<R, E>>,
{
    type Item = R;

    fn next(&mut self) -> Option<R> {
        match self.iter.next()? {
            Err(err) => {
                self.outcome = Err(err);
                None
            }
            Ok(res) => Some(res),
        }
    }
}
