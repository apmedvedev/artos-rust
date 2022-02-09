use crate::artos::server::channel::{Context, ServerState};
use uuid::Uuid;

#[cfg_attr(test, mockall::automock)]
pub trait CompletionHandler<T: 'static, E: 'static> {
    fn on_succeeded(&self, result: T);

    fn on_failed(&self, error: E);
}

pub fn update_term(context: &Context, server_state: &mut ServerState, term: u64) -> bool {
    if term > server_state.persistent.term() {
        server_state.persistent.set_term(term);
        server_state.persistent.set_voted_for(None);

        let transaction = context.state_machine().begin_transaction(false);
        transaction.write_state(server_state.persistent);
        transaction.commit();

        true
    } else {
        false
    }
}

pub fn to_string<T>(_request: &T, _server_id: Uuid, _state: &ServerState, _additional_fields: bool) -> String {
    //TODO:
    todo!()
    //return message.toString(getPeerEndpoint(state.getPeers(), message.getSource()), additionalFields);
}

pub fn to_string_id(_server_id: Uuid, _server_state: &ServerState) -> String {
    todo!()
}
