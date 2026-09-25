use super::access_and_filters::{QueryFilter, TransferableQueryFilter};
use super::traits_and_state::{QueryState, TransferableQueryData};
use crate::world::WorkerWorldBuilder;

pub(crate) fn prepare_query<Q, F>(
    state: &QueryState<Q, F>,
    builder: &mut WorkerWorldBuilder<'_>,
)
where
    Q: TransferableQueryData + 'static,
    F: QueryFilter + TransferableQueryFilter + 'static,
{
    if state.prepare_worker_projection(builder) {
        return;
    }
    let mut required_present = Q::query_types();
    let mut filter_required = Vec::new();
    let mut excluded = Vec::new();
    F::configure(&mut filter_required, &mut excluded);
    for type_id in filter_required {
        if !required_present.contains(&type_id) {
            required_present.push(type_id);
        }
    }
    for type_id in required_present {
        builder.prepare_membership(type_id);
    }
    for type_id in excluded {
        builder.prepare_membership(type_id);
    }
    Q::prepare_worker(builder);
    F::prepare_worker(builder);
}
