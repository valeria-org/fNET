# Mono opcode source audit

The audit covers all 46 commits returned by the mono/mono main-branch history
for mono/cil/opcode.def, from 2001-09-05 through 2020-08-10, and the current
dotnet/runtime table and JIT consumer pinned to
b9bafb9d14ef9e02a3940f97164b6b231d3595bb.

mono-opcode-history-commits.json records the complete commit list and author
timestamps. mono-opcode-history-audit.tsv records every active CEE_MONO_
declaration at those revisions; the initial September 2001 table has no Mono
entries. The TSV deliberately preserves declarations, including historical
table errors. It is a provenance catalog, not an authoritative width fixture.

## Stable conflicting maps

| Snapshot | SHA | Author timestamp UTC | Saved source |
|-|-|-|-|
| Before PROC2 insertion | e9af813033456c0be99842ac680444cdfc38bb5a | 2002-07-19 13:29:27 | mono-july2002-opcode.def |
| Before FUNC2 insertion | 6d80e780be940b906044ff2777f32e73e1e3cbff | 2004-03-05 03:44:06 | mono-early2002-opcode.def |
| Before ICALL replacement | f284ad8dc662c6e5dc1c68c0d08926c927d844bf | 2004-05-20 12:48:40 | mono-may2004-opcode.def |
| Late legacy error getter | 2cee7211900fda7f3a19171dc6f484520dce820f | 2018-09-18 20:03:56 | mono-legacy-getter-opcode.def |

The initial 2002 map acquired VTADDR on July 18 and NEWOBJ on July 19.
33190343e99ae46041b4e836875870731b720a16 (2002-07-24) inserted PROC2,
renumbering the subsequent instructions. RETOBJ followed on August 1,
LDNATIVEOBJ on 2003-01-29, and CISINST/CCASTCLASS on 2004-03-05.
becfd57d95eb36c84953ee9cc6d640614f9b4cbe (2004-05-11) inserted FUNC2,
renumbering subsequent instructions again. SAVE_LMF/RESTORE_LMF followed on
May 20. 06093c36a5618d1c40a265b40fbf421153ae50be (2004-07-08)
replaced the conversion helpers with ICALL and established the modern map.
These maps conflict and cannot be reliably autodetected.

F0 1B changed from GET_LAST_ERROR to SAVE_LAST_ERROR at
b1b61d7cb7f580f5e5438a756ef3e67a5a832eeb (2019-07-05).
Both encodings have no payload, but the getter pushes one integer whereas the
saver pushes nothing. No other identity reassignment was found after the 2004
ICALL replacement. JIT_ATTACH/JIT_DETACH were retired at
766dacef2240ac046723e80aaba2cfbbf9caad90 (2018-09-12).

## Verified layouts and metadata corrections

FUNC1/FUNC2/PROC2/PROC3 consume an unsigned one-byte conversion identifier.
The May 2004 marshal.c emitter emits FUNC2 followed by emit_byte(ASANY);
mini.c checks size 3, switches on ip[2], consumes two stack values, produces
one, and advances by three. The saved mono-audit-f284ad8d sources preserve
this evidence. Other historical pointer/type operands are four-byte wrapper
data indexes, not native-width pointer immediates. CISINST/CCASTCLASS each
consume exactly one such index, total six bytes.

The late getter emitter in mono-legacy-getter-marshal-ilgen.c emits F0 and
GET_LAST_ERROR with no payload, immediately followed by an ICALL setter.
mono-legacy-getter-method-to-ir.c emits OP_GET_LAST_ERROR and pushes its
STACK_I4 result. The 2016 consumer also explicitly advances by two.

Historical ICALL declarations say ShortInlineI, but the 2005, 2009, and 2013
consumers read32(ip+2) and advance by six. Historical DYN_CALL declarations
say InlineI, but both emitter and consumer use only two opcode bytes.
Historical JIT_ICALL_ADDR declarations say InlineNone, but the emitter and
consumer use four operand bytes. 822f75e8c7a4a0bcdfd7f143d46fd95066b454eb
(2018-05-11) explicitly fixes ICALL and DYN_CALL table sizing.

The 2013 MEMORY_BARRIER emitter uses F0 plus emit_op, which appends four bytes.
Its consumer's read32(ip+1)/advance-by-five is a source bug. Current
sgen-mono-ilgen.c emits F0/opcode plus emit_i4(barrier_kind), also six bytes.
The old emit_op stored a wrapper-data index; the current payload is a barrier
kind. This semantic difference does not change decoding width.

The pinned modern consumer is preserved in mono-current-method-to-ir.c.
All modern Mono operand widths agree with its table-driven decode. Fixed
stack counts agree except DYN_CALL, which consumes two and produces zero.
LDNATIVEOBJ falls through to the next instruction despite the table's RETURN
classification. Modern JIT handlers for CISINST/CCASTCLASS are absent while
their opcode declarations remain available for historical disassembly.

ICALL and DYN_CALL semantically invoke calls although the Mono table classifies
them NEXT. Reporting Call flow for those two is a semantic API choice, not an
encoded-layout correction. No non-F0 Mono namespace was demonstrated by this
history audit. This audit does not claim coverage of unrelated interpreter
wordcode, JIT IR, vendor forks, or arbitrary experimental opcode definitions.

Full consumer/emitter C snapshots are retained in the checkout as research
references under their original licensing; they are omitted from the Cargo
package and are not part of the Rust implementation. Historical COPYING.LIB
and the later Mono license manifest are saved alongside the source snapshots.
