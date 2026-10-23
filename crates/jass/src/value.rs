use std::fmt;
use std::rc::Rc;

/// Index of a compiled JASS function.
pub type FuncId = u32;

#[derive(Clone, Default, PartialEq)]
pub enum Value {
    /// A scalar variable that was never assigned; reading it aborts the thread.
    #[default]
    Uninit,
    Null,
    Int(i32),
    Real(f32),
    Bool(bool),
    Str(Rc<str>),
    Handle(u32),
    Code(FuncId),
}

impl Value {
    pub fn int(&self) -> i32 {
        match *self {
            Value::Int(v) => v,
            Value::Real(v) => v as i32,
            Value::Bool(b) => b as i32,
            Value::Handle(h) => h as i32,
            Value::Code(c) => c as i32,
            _ => 0,
        }
    }
    pub fn real(&self) -> f32 {
        match *self {
            Value::Real(v) => v,
            Value::Int(v) => v as f32,
            _ => 0.0,
        }
    }
    pub fn bool(&self) -> bool {
        match *self {
            Value::Bool(b) => b,
            Value::Int(v) => v != 0,
            _ => false,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn handle(&self) -> u32 {
        match *self {
            Value::Handle(h) => h,
            _ => 0,
        }
    }
    pub fn code(&self) -> Option<FuncId> {
        match *self {
            Value::Code(c) => Some(c),
            _ => None,
        }
    }
    pub fn string(s: impl AsRef<str>) -> Value {
        Value::Str(Rc::from(s.as_ref()))
    }
    pub fn opt_handle(h: u32) -> Value {
        if h == 0 { Value::Null } else { Value::Handle(h) }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Uninit => write!(f, "<uninit>"),
            Value::Null => write!(f, "null"),
            Value::Int(v) => write!(f, "{v}"),
            Value::Real(v) => write!(f, "{v:?}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Handle(h) => write!(f, "handle#{h:#x}"),
            Value::Code(c) => write!(f, "function#{c}"),
        }
    }
}

/// The primitive type a declared JASS type reduces to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseType {
    Integer,
    Real,
    Boolean,
    String,
    Code,
    Handle,
}

impl BaseType {
    pub fn default_value(self) -> Value {
        match self {
            BaseType::Integer => Value::Int(0),
            BaseType::Real => Value::Real(0.0),
            BaseType::Boolean => Value::Bool(false),
            _ => Value::Null,
        }
    }
}
