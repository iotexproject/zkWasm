use crate::constant;

use super::*;
use halo2_proofs::{arithmetic::FieldExt, plonk::ConstraintSystem};

pub(super) mod op_bin;
pub(super) mod op_br_if;
pub(super) mod op_const;
pub(super) mod op_drop;
pub(super) mod op_local_get;
pub(super) mod op_local_set;
pub(super) mod op_local_tee;
pub(super) mod op_return;
pub(super) mod op_load;
pub(super) mod op_rel;

// TODO: replace repeated code with macro

pub struct Cell {
    pub col: Column<Advice>,
    pub rot: i32,
}

impl Cell {
    pub fn assign<F: FieldExt>(&self, ctx: &mut Context<'_, F>, value: F) -> Result<(), Error> {
        ctx.region.assign_advice(
            || "cell",
            self.col,
            (ctx.offset as i32 + self.rot) as usize,
            || Ok(value),
        )?;
        Ok(())
    }

    pub fn expr<F: FieldExt>(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        nextn!(meta, self.col, self.rot)
    }
}

pub struct MTableLookupCell {
    pub col: Column<Advice>,
    pub rot: i32,
}

impl MTableLookupCell {
    pub fn assign<F: FieldExt>(
        &self,
        ctx: &mut Context<'_, F>,
        value: &BigUint,
    ) -> Result<(), Error> {
        ctx.region.assign_advice(
            || "mlookup cell",
            self.col,
            (ctx.offset as i32 + self.rot) as usize,
            || Ok(bn_to_field(value)),
        )?;
        Ok(())
    }

    pub fn expr<F: FieldExt>(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        nextn!(meta, self.col, self.rot)
    }
}

pub struct JTableLookupCell {
    pub col: Column<Advice>,
    pub rot: i32,
}

impl JTableLookupCell {
    pub fn assign<F: FieldExt>(
        &self,
        ctx: &mut Context<'_, F>,
        value: &BigUint,
    ) -> Result<(), Error> {
        ctx.region.assign_advice(
            || "jlookup cell",
            self.col,
            (ctx.offset as i32 + self.rot) as usize,
            || Ok(bn_to_field(value)),
        )?;
        Ok(())
    }

    pub fn expr<F: FieldExt>(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        nextn!(meta, self.col, self.rot)
    }
}

pub struct BitCell {
    pub col: Column<Advice>,
    pub rot: i32,
}

impl BitCell {
    pub fn assign<F: FieldExt>(&self, ctx: &mut Context<'_, F>, value: bool) -> Result<(), Error> {
        ctx.region.assign_advice(
            || "bit cell",
            self.col,
            (ctx.offset as i32 + self.rot) as usize,
            || Ok(F::from(value as u64)),
        )?;

        Ok(())
    }

    pub fn expr<F: FieldExt>(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        nextn!(meta, self.col, self.rot)
    }
}

pub struct CommonRangeCell {
    pub col: Column<Advice>,
    pub rot: i32,
}

impl CommonRangeCell {
    pub fn assign<F: FieldExt>(&self, ctx: &mut Context<'_, F>, value: u16) -> Result<(), Error> {
        ctx.region.assign_advice(
            || "common range cell",
            self.col,
            (ctx.offset as i32 + self.rot) as usize,
            || Ok(F::from(value as u64)),
        )?;
        Ok(())
    }

    pub fn expr<F: FieldExt>(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        nextn!(meta, self.col, self.rot)
    }
}

pub struct U64Cell {
    pub value_col: Column<Advice>,
    pub value_rot: i32,
    pub u4_col: Column<Advice>,
}

impl U64Cell {
    pub fn assign<F: FieldExt>(
        &self,
        ctx: &mut Context<'_, F>,
        mut value: u64,
    ) -> Result<(), Error> {
        ctx.region.assign_advice(
            || "u64 range cell",
            self.value_col,
            (ctx.offset as i32 + self.value_rot) as usize,
            || Ok(F::from(value)),
        )?;

        for i in 0..16usize {
            let v = value & 0xf;
            value >>= 4;
            ctx.region.assign_advice(
                || "u4 range cell",
                self.u4_col,
                ctx.offset + i,
                || Ok(F::from(v)),
            )?;
        }

        Ok(())
    }

    pub fn expr<F: FieldExt>(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        nextn!(meta, self.value_col, self.value_rot)
    }
}

pub(super) struct EventTableCellAllocator<'a, F> {
    pub config: &'a EventTableCommonConfig<F>,
    pub bit_index: i32,
    pub common_range_index: i32,
    pub unlimit_index: i32,
    pub u64_index: i32,
    pub mtable_lookup_index: i32,
    pub jtable_lookup_index: i32,
}

impl<'a, F: FieldExt> EventTableCellAllocator<'a, F> {
    pub(super) fn new(config: &'a EventTableCommonConfig<F>) -> Self {
        Self {
            config,
            bit_index: EventTableBitColumnRotation::Max as i32,
            common_range_index: EventTableCommonRangeColumnRotation::Max as i32,
            unlimit_index: EventTableUnlimitColumnRotation::SharedStart as i32,
            u64_index: 0,
            mtable_lookup_index: EventTableUnlimitColumnRotation::MTableLookupStart as i32,
            jtable_lookup_index: EventTableUnlimitColumnRotation::JTableLookup as i32,
        }
    }

    pub fn alloc_bit_value(&mut self) -> BitCell {
        assert!(self.bit_index < ETABLE_STEP_SIZE as i32);
        let allocated_index = self.bit_index;
        self.bit_index += 1;
        BitCell {
            col: self.config.shared_bits,
            rot: allocated_index,
        }
    }

    pub fn alloc_common_range_value(&mut self) -> CommonRangeCell {
        assert!(self.common_range_index < ETABLE_STEP_SIZE as i32);
        let allocated_index = self.common_range_index;
        self.common_range_index += 1;
        CommonRangeCell {
            col: self.config.state,
            rot: allocated_index,
        }
    }

    pub fn alloc_unlimited_value(&mut self) -> Cell {
        assert!(self.unlimit_index < ETABLE_STEP_SIZE as i32);
        let allocated_index = self.unlimit_index;
        self.unlimit_index += 1;
        Cell {
            col: self.config.aux,
            rot: allocated_index,
        }
    }

    pub fn alloc_u64(&mut self) -> U64Cell {
        assert!(self.u64_index < U4_COLUMNS as i32);
        let allocated_index = self.u64_index;
        self.u64_index += 1;
        U64Cell {
            value_col: self.config.aux,
            value_rot: allocated_index + EventTableUnlimitColumnRotation::U64Start as i32,
            u4_col: self.config.u4_shared[allocated_index as usize],
        }
    }

    pub fn alloc_mtable_lookup(&mut self) -> MTableLookupCell {
        assert!(self.mtable_lookup_index < EventTableUnlimitColumnRotation::U64Start as i32);
        let allocated_index = self.mtable_lookup_index;
        self.mtable_lookup_index += 1;
        MTableLookupCell {
            col: self.config.aux,
            rot: allocated_index,
        }
    }

    pub fn alloc_jtable_lookup(&mut self) -> JTableLookupCell {
        assert!(
            self.jtable_lookup_index < EventTableUnlimitColumnRotation::MTableLookupStart as i32
        );
        let allocated_index = self.jtable_lookup_index;
        self.jtable_lookup_index += 1;
        JTableLookupCell {
            col: self.config.aux,
            rot: allocated_index,
        }
    }
}

pub(super) trait EventTableOpcodeConfigBuilder<F: FieldExt> {
    fn configure(
        meta: &mut ConstraintSystem<F>,
        common: &mut EventTableCellAllocator<F>,
        enable: impl Fn(&mut VirtualCells<'_, F>) -> Expression<F>,
    ) -> Box<dyn EventTableOpcodeConfig<F>>;
}

pub(super) trait EventTableOpcodeConfig<F: FieldExt> {
    fn opcode(&self, meta: &mut VirtualCells<'_, F>) -> Expression<F>;
    fn opcode_class(&self) -> OpcodeClass;

    fn assign(
        &self,
        ctx: &mut Context<'_, F>,
        step: &StepStatus,
        entry: &EventTableEntry,
    ) -> Result<(), Error>;

    fn sp_diff(&self, _meta: &mut VirtualCells<'_, F>) -> Option<Expression<F>> {
        None
    }

    fn jops(&self, _meta: &mut VirtualCells<'_, F>) -> Option<Expression<F>> {
        None
    }
    fn mops(&self, _meta: &mut VirtualCells<'_, F>) -> Option<Expression<F>> {
        None
    }
    fn next_last_jump_eid(
        &self,
        _meta: &mut VirtualCells<'_, F>,
        _common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        None
    }
    fn next_moid(
        &self,
        _meta: &mut VirtualCells<'_, F>,
        _common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        None
    }
    fn next_fid(
        &self,
        _meta: &mut VirtualCells<'_, F>,
        _common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        None
    }
    fn next_iid(
        &self,
        meta: &mut VirtualCells<'_, F>,
        common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        Some(common_config.iid(meta) + constant_from!(1))
    }
    fn mtable_lookup(
        &self,
        _meta: &mut VirtualCells<'_, F>,
        _item: MLookupItem,
        _common: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        None
    }
    fn jtable_lookup(
        &self,
        _meta: &mut VirtualCells<'_, F>,
        _common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        None
    }
    fn itable_lookup(
        &self,
        _meta: &mut VirtualCells<'_, F>,
        _common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        None
    }
}
