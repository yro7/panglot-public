use static_assertions::assert_impl_all;

use crate::{
    AddNodeRequest, AddNodeResponse, AppState, ExportResponse, GenerateRequest,
    GenerateResponse, GeneratedCardJson, GetTreeQuery, PreviewPromptRequest,
    PreviewPromptResponse, PromptMessageJson, TreeNodeJson,
};

// ── App-level types ──
assert_impl_all!(AppState: Send, Sync);

// ── API request/response types ──
assert_impl_all!(TreeNodeJson: Send, Sync);
assert_impl_all!(GeneratedCardJson: Send, Sync);
assert_impl_all!(GenerateRequest: Send, Sync);
assert_impl_all!(GenerateResponse: Send, Sync);
assert_impl_all!(ExportResponse: Send, Sync);
assert_impl_all!(PreviewPromptRequest: Send, Sync);
assert_impl_all!(PreviewPromptResponse: Send, Sync);
assert_impl_all!(PromptMessageJson: Send, Sync);
assert_impl_all!(AddNodeRequest: Send, Sync);
assert_impl_all!(AddNodeResponse: Send, Sync);
assert_impl_all!(GetTreeQuery: Send, Sync);
