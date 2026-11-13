//! The annotation entities: MTEXT, dimensions, leaders, hatches and the rest.

use draftcraft_dwg::compose::ObjectWriter;

use super::Cx;
use super::rec::Rec;

/// One entity of the types this module writes. `None` for a type the writer has no encoding for.
pub(super) fn entity(_cx: &Cx<'_>, _r: &Rec<'_>) -> Option<ObjectWriter> {
    None
}
