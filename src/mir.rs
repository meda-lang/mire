use std::collections::HashMap;
use std::ffi::CString;

use index_vec::{IndexVec, define_index_type};
use smallvec::SmallVec;
use string_interner::DefaultSymbol as Sym;

use crate::hir::{Intrinsic, StructId, ModScopeId};
use crate::ty::Type;
use crate::{Code, BlockId, OpId, Block};

define_index_type!(pub struct FuncId = u32;);
define_index_type!(pub struct InstrId = u32;);
define_index_type!(pub struct StaticId = u32;);
define_index_type!(pub struct StrId = u32;);

#[derive(Debug, PartialEq)]
pub enum Instr {
    Void,
    Const(Const),
    Alloca(Type),
    LogicalNot(InstrId),
    Call { arguments: SmallVec<[InstrId; 2]>, func: FuncId },
    Intrinsic { arguments: SmallVec<[InstrId; 2]>, ty: Type, intr: Intrinsic },
    Reinterpret(InstrId, Type),
    Truncate(InstrId, Type),
    SignExtend(InstrId, Type),
    ZeroExtend(InstrId, Type),
    FloatCast(InstrId, Type),
    FloatToInt(InstrId, Type),
    IntToFloat(InstrId, Type),
    Load(InstrId),
    Store { location: InstrId, value: InstrId },
    AddressOfStatic(StaticId),
    Pointer { op: InstrId, is_mut: bool },
    Struct { fields: SmallVec<[InstrId; 2]>, id: StructId },
    StructLit { fields: SmallVec<[InstrId; 2]>, id: StructId },
    DirectFieldAccess { val: InstrId, index: usize },
    IndirectFieldAccess { val: InstrId, index: usize },
    Ret(InstrId),
    Br(BlockId),
    CondBr { condition: InstrId, true_bb: BlockId, false_bb: BlockId },
    /// Only valid at the beginning of a function, right after the void instruction
    Parameter(Type),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Const {
    Int { lit: u64, ty: Type },
    Float { lit: f64, ty: Type },
    Str { id: StrId, ty: Type },
    Bool(bool),
    Ty(Type),
    Mod(ModScopeId),
    StructLit { fields: Vec<Const>, id: StructId },
}

impl Const {
    pub fn ty(&self) -> Type {
        match self {
            Const::Int { ty, .. } => ty.clone(),
            Const::Float { ty, .. } => ty.clone(),
            Const::Str { ty, .. } => ty.clone(),
            Const::Bool(_) => Type::Bool,
            Const::Ty(_) => Type::Ty,
            Const::Mod(_) => Type::Mod,
            &Const::StructLit { id, .. } => Type::Struct(id),
        }
    }
}

#[derive(Debug, Default)]
pub struct Function {
    pub name: Option<Sym>,
    pub ret_ty: Type,
    /// Index 0 is defined to be the entry block
    pub blocks: Vec<BlockId>,
}

mod private {
    pub trait Sealed {}
}

pub trait GetBlock<'a>: private::Sealed {
    fn get_block(self, code: &'a Code) -> &'a Block;
}

impl private::Sealed for BlockId {}
impl<'a> GetBlock<'a> for BlockId {
    fn get_block(self, code: &'a Code) -> &'a Block {
        &code.blocks[self]
    }
}

impl private::Sealed for &Block {}
impl<'a> GetBlock<'a> for &'a Block {
    fn get_block(self, _code: &'a Code) -> &'a Block {
        self
    }
}

impl Code {
    pub fn get_mir_instr<'a>(&'a self, block: impl GetBlock<'a>, op: OpId) -> Option<&'a Instr> {
        let block = block.get_block(self);
        block.ops[op].as_mir_instr().map(|instr| &self.mir_code.instrs[instr])
    }

    pub fn num_parameters(&self, func: &Function) -> usize {
        let entry = func.blocks[0];
        let block = &self.blocks[entry];
        let void_instr = self.get_mir_instr(block, OpId::new(0)).unwrap();
        assert_eq!(void_instr, &Instr::Void);
        let mut num_parameters = 0;
        for i in 1..block.ops.len() {
            match self.get_mir_instr(block, OpId::new(i)).unwrap() {
                Instr::Parameter(_) => num_parameters += 1,
                _ => break,
            }
        }
        num_parameters
    }
}

#[derive(Clone)]
pub struct Struct {
    pub field_tys: SmallVec<[Type; 2]>,
    pub layout: StructLayout,
}

#[derive(Clone)]
pub struct StructLayout {
    pub field_offsets: SmallVec<[usize; 2]>,
    pub alignment: usize,
    pub size: usize,
    pub stride: usize,
}

#[derive(Debug)]
pub enum BlockState {
    Created,
    Started,
    Ended,
}

#[derive(Default)]
pub struct MirCode {
    pub strings: IndexVec<StrId, CString>,
    pub functions: IndexVec<FuncId, Function>,
    pub statics: IndexVec<StaticId, Const>,
    pub structs: HashMap<StructId, Struct>,
    pub instrs: IndexVec<InstrId, Instr>,
    block_states: HashMap<BlockId, BlockState>,
}

impl MirCode {
    fn get_block_state(&mut self, block: BlockId) -> &mut BlockState {
        self.block_states.entry(block).or_insert(BlockState::Created)
    }

    pub fn start_block(&mut self, block: BlockId) {
        let state = self.get_block_state(block);
        assert!(!matches!(state, BlockState::Ended), "MIR: tried to start an ended block");
        *state = BlockState::Started;
    }

    pub fn end_block(&mut self, block: BlockId) {
        let state = self.get_block_state(block);
        assert!(matches!(state, BlockState::Started), format!("MIR: tried to end a block in the {:?} state", *state));
    }

    pub fn check_all_blocks_ended(&self, func: &Function) {
        for &block in &func.blocks {
            let state = &self.block_states[&block];
            assert!(matches!(state, BlockState::Ended), format!("Block {} was not ended", block.index()));
        }
    }
}
