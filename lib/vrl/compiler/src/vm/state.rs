use super::{argument_list::VmArgument, machine::Instruction, OpCode, Vm, VmFunctionClosure};
use crate::{ExpressionError, Value};

/// `VmState` contains the mutable state used to run the Vm.
pub(crate) struct VmState<'a> {
    vm: &'a Vm,
    /// The index of the next instruction to run.
    pub(super) instruction_pointer: usize,
    /// The stack used to store values whilst running the process.
    pub(super) stack: Vec<Value>,
    /// A stack of values passed to functions when called.
    pub(super) parameter_stack: Vec<Option<VmArgument<'a>>>,
    /// A stack of closures.
    pub(super) closure_stack: Vec<&'a VmFunctionClosure>,
    /// Errors generated by the last expression are stored here.
    pub(super) error: Option<ExpressionError>,
}

impl<'a> VmState<'a> {
    pub(super) fn new(vm: &'a Vm) -> Self {
        Self {
            vm,
            instruction_pointer: 0,
            stack: Vec::new(),
            parameter_stack: Vec::new(),
            closure_stack: Vec::new(),
            error: None,
        }
    }

    /// Return the `Opcode` at the current position and advance the instruction pointer.
    /// Returns an error if the current position is not an OpCode.
    /// This error should never occur for a correctly compiled program!
    pub(super) fn next_opcode(&mut self) -> Result<OpCode, ExpressionError> {
        let byte = self.vm.instructions()[self.instruction_pointer];
        self.instruction_pointer += 1;
        match byte {
            Instruction::OpCode(opcode) => Ok(opcode),
            _ => Err(format!("Expecting opcode at {}", self.instruction_pointer - 1).into()),
        }
    }

    /// Return the primitive value at the current position and advance the instruction pointer.
    /// Returns an error if the current position is not a primitve value.
    /// This error should never occur for a correctly compiled program!
    pub(super) fn next_primitive(&mut self) -> Result<usize, ExpressionError> {
        let byte = self.vm.instructions()[self.instruction_pointer];
        self.instruction_pointer += 1;
        match byte {
            Instruction::Primitive(primitive) => Ok(primitive),
            _ => Err(format!("Expecting primitive at {}", self.instruction_pointer - 1).into()),
        }
    }

    /// Pushes the given value onto the stack.
    pub(super) fn push_stack(&mut self, value: Value) {
        self.stack.push(value);
    }

    /// Pops the value from the top of the stack.
    /// Errors if the stack is empty.
    pub(super) fn pop_stack(&mut self) -> Result<Value, ExpressionError> {
        self.stack.pop().ok_or_else(|| "stack underflow".into())
    }

    /// Pops the closure from the top of the stack.
    /// Errors if the stack is empty.
    #[cfg(feature = "expr-function_call")]
    pub(super) fn pop_closure(&mut self) -> Result<&VmFunctionClosure, ExpressionError> {
        self.closure_stack
            .pop()
            .ok_or_else(|| "closure stack underflow".into())
    }

    pub(super) fn peek_stack(&self) -> Result<&Value, ExpressionError> {
        if self.stack.is_empty() {
            return Err("peeking empty stack".into());
        }

        Ok(&self.stack[self.stack.len() - 1])
    }

    #[cfg(feature = "expr-function_call")]
    pub(super) fn parameter_stack(&self) -> &Vec<Option<VmArgument<'a>>> {
        &self.parameter_stack
    }

    #[cfg(feature = "expr-function_call")]
    pub(super) fn parameter_stack_mut(&mut self) -> &mut Vec<Option<VmArgument<'a>>> {
        &mut self.parameter_stack
    }

    pub(super) fn read_constant(&mut self) -> Result<Value, ExpressionError> {
        let idx = self.next_primitive()?;
        Ok(self.vm.values()[idx].clone())
    }
}
