use std::fmt;

#[derive(Debug, Clone)]
pub struct MalformattedGeneFamilyError;

impl fmt::Display for MalformattedGeneFamilyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Malformatted gene family")
    }
}