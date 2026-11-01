// A fact write takes the caller's transaction, never a bare connection, so the
// caller decides where the transaction begins and ends (I-9). This file must
// fail to compile.

use pos_db::repo::audit::AuditRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_domain::{AuditIntent, RegisterId};
use rusqlite::Connection;
use uuid::Uuid;

fn audit_on_the_bare_connection(conn: &Connection, register: RegisterId, intent: &AuditIntent) {
    let _ = AuditRepository::new(conn).append(conn, register, Uuid::nil(), intent);
}

fn commit_a_manifest_on_the_bare_connection(
    conn: &Connection,
    envelope: &CommitEnvelope<'_>,
    members: &[FactMember<'_>],
) {
    let _ = OutboxRepository::new(conn).write_commit(conn, envelope, members);
}

fn main() {
    let _ = (audit_on_the_bare_connection, commit_a_manifest_on_the_bare_connection);
}
