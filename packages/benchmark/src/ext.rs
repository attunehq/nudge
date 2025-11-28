use std::borrow::Cow;

use ::indent::indent_all_by;
use extfn::extfn;

/// Indent all non-empty lines by the given number of spaces.
#[extfn]
pub fn indent<'a>(self: impl Into<Cow<'a, str>>, level: usize) -> String {
    indent_all_by(level, self)
}
