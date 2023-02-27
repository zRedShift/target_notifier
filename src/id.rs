pub static INCORRECT_INDEX: &str = "Incorrect channel index";

#[derive(Clone, Copy, PartialEq)]
pub struct ID(
    pub(super) usize,
    pub(super) Option<usize>,
    pub(super) &'static str,
);
impl ID {
    pub fn new(id: usize) -> Self {
        Self(id, None, "")
    }
    pub fn id(&self) -> usize {
        self.0
    }
    pub fn index(&self) -> Option<usize> {
        self.1
    }
    pub fn name(&self) -> &'static str {
        self.2
    }
    pub fn set_index(mut self, index: usize) -> Self {
        self.1 = Some(index);
        self
    }
    pub fn set_name(mut self, name: &'static str) -> Self {
        self.2 = name;
        self
    }
    pub(super) fn eq_target(&self, other: &Self) -> bool {
        match other.1 {
            Some(_) => self.eq(other),
            None => self.0 == other.0,
        }
    }
}

impl core::fmt::Display for ID {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match (self.0, self.1) {
            (usize::MAX, _) => write!(f, "[{}]", self.2),
            (id, None) => write!(f, "[{}](Id: {id})", self.2),
            (id, Some(index)) => write!(f, "[{}({index})](Id: {id})", self.2),
        }
    }
}
