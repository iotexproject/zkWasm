use halo2_proofs::{
    arithmetic::FieldExt,
    plonk::{Error, Expression, VirtualCells},
};
use num_bigint::BigUint;
use specs::{
    etable::EventTableEntry,
    host_function::HostPlugin,
    itable::{OpcodeClass, OPCODE_CLASS_SHIFT},
    mtable::{LocationType, VarType},
    step::StepInfo,
};

use crate::{
    circuits::{
        cell::{AllocatedBitCell, AllocatedU64Cell, CellExpression},
        etable_v2::{
            allocator::{
                AllocatedMemoryTableLookupReadCell, AllocatedMemoryTableLookupWriteCell,
                EventTableCellAllocator,
            },
            constraint_builder::ConstraintBuilder,
            EventTableCommonConfig, EventTableOpcodeConfig,
        },
        utils::{
            bn_to_field, step_status::StepStatus, table_entry::EventTableEntryWithMemoryInfo,
            Context,
        },
    },
    constant_from, constant_from_bn,
    foreign::v2::{EventTableForeignCallConfigBuilder, InternalHostPluginBuilder},
};

use super::circuits::{InputTableEncode, WASM_INPUT_FOREIGN_TABLE_KEY};

pub struct ETableWasmInputHelperTableConfig<F: FieldExt> {
    plugin_index: usize,

    is_public: AllocatedBitCell<F>,
    value: AllocatedU64Cell<F>,

    lookup_read_stack: AllocatedMemoryTableLookupReadCell<F>,
    lookup_write_stack: AllocatedMemoryTableLookupWriteCell<F>,
}

pub struct ETableWasmInputHelperTableConfigBuilder {
    index: usize,
}

impl InternalHostPluginBuilder for ETableWasmInputHelperTableConfigBuilder {
    fn new(index: usize) -> Self {
        Self { index }
    }
}

impl<F: FieldExt> EventTableForeignCallConfigBuilder<F>
    for ETableWasmInputHelperTableConfigBuilder
{
    fn configure(
        self,
        common_config: &EventTableCommonConfig<F>,
        allocator: &mut EventTableCellAllocator<F>,
        constraint_builder: &mut ConstraintBuilder<F>,
    ) -> Box<dyn EventTableOpcodeConfig<F>> {
        let eid = common_config.eid_cell;
        let sp = common_config.sp_cell;
        let public_input_index = common_config.input_index_cell;

        let is_public = allocator.alloc_bit_cell();
        let value = allocator.alloc_u64_cell();
        let lookup_read_stack = allocator.alloc_memory_table_lookup_read_cell(
            "wasm input stack read",
            constraint_builder,
            eid,
            move |_| constant_from!(LocationType::Stack as u64),
            move |meta| sp.expr(meta) + constant_from!(1),
            move |meta| constant_from!(1),
            move |meta| is_public.expr(meta),
            move |_| constant_from!(1),
        );
        let lookup_write_stack = allocator.alloc_memory_table_lookup_write_cell(
            "wasm input stack write",
            constraint_builder,
            eid,
            move |_| constant_from!(LocationType::Stack as u64),
            move |meta| sp.expr(meta) + constant_from!(1),
            move |meta| constant_from!(0),
            move |meta| value.u64_cell.expr(meta),
            move |_| constant_from!(1),
        );

        constraint_builder.lookup(
            WASM_INPUT_FOREIGN_TABLE_KEY,
            "lookup input table",
            Box::new(move |meta| {
                is_public.expr(meta)
                    * InputTableEncode::encode_for_lookup(
                        public_input_index.expr(meta),
                        value.u64_cell.expr(meta),
                    )
            }),
        );

        Box::new(ETableWasmInputHelperTableConfig {
            plugin_index: self.index,
            is_public,
            value,
            lookup_read_stack,
            lookup_write_stack,
        })
    }
}

impl<F: FieldExt> EventTableOpcodeConfig<F> for ETableWasmInputHelperTableConfig<F> {
    fn opcode(&self, _meta: &mut VirtualCells<'_, F>) -> Expression<F> {
        constant_from_bn!(
            &(BigUint::from(OpcodeClass::ForeignPluginStart as u64 + self.plugin_index as u64)
                << OPCODE_CLASS_SHIFT)
        )
    }

    fn assign(
        &self,
        ctx: &mut Context<'_, F>,
        step: &StepStatus,
        entry: &EventTableEntryWithMemoryInfo,
    ) -> Result<(), Error> {
        match &entry.eentry.step_info {
            StepInfo::CallHost {
                plugin,
                args,
                ret_val,
                signature,
                ..
            } => {
                let arg_type: VarType = (*signature.params.get(0).unwrap()).into();
                let ret_type: VarType = signature.return_type.unwrap().into();

                assert_eq!(args.len(), 1);
                assert_eq!(arg_type, VarType::I32);
                assert_eq!(ret_type, VarType::I64);

                self.is_public
                    .assign(ctx, F::from(*args.get(0).unwrap() == 1))?;
                self.value.assign(ctx, ret_val.unwrap())?;

                self.lookup_read_stack.assign(
                    ctx,
                    entry.memory_rw_entires[0].start_eid,
                    step.current.eid,
                    entry.memory_rw_entires[0].end_eid,
                    step.current.sp + 1,
                    LocationType::Stack,
                    true,
                    *args.get(0).unwrap(),
                )?;

                self.lookup_write_stack.assign(
                    ctx,
                    step.current.eid,
                    entry.memory_rw_entires[1].end_eid,
                    step.current.sp + 1,
                    LocationType::Stack,
                    false,
                    ret_val.unwrap(),
                )?;

                Ok(())
            }

            _ => unreachable!(),
        }
    }

    fn mops(&self, _meta: &mut VirtualCells<'_, F>) -> Option<Expression<F>> {
        Some(constant_from!(1))
    }

    fn memory_writing_ops(&self, entry: &EventTableEntry) -> u32 {
        1
    }

    fn input_index_increase(
        &self,
        meta: &mut VirtualCells<'_, F>,
        _common_config: &EventTableCommonConfig<F>,
    ) -> Option<Expression<F>> {
        Some(self.is_public.expr(meta))
    }

    fn is_host_public_input(&self, entry: &EventTableEntry) -> bool {
        match &entry.step_info {
            StepInfo::CallHost { plugin, args, .. } => {
                assert_eq!(*plugin, HostPlugin::HostInput);
                assert_eq!(args.len(), 1);
                args[0] != 0
            }
            _ => unreachable!(),
        }
    }
}