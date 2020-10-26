use crate::{Expression, Object, Program, Result, State, Value};

pub struct Runtime {
    state: State,
}

impl Runtime {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    /// Given the provided [`Object`], run the provided [`Program`] to
    /// completion.
    pub fn execute(&mut self, mut object: impl Object, program: Program) -> Result<Option<Value>> {
        let mut values = program
            .expressions
            .iter()
            .map(|expression| expression.execute(&mut self.state, &mut object))
            .collect::<Result<Vec<Option<Value>>>>()?;

        Ok(values.pop().flatten())
    }
}
