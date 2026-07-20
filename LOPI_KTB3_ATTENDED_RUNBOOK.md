# KT-B3 attended runbook — real Claude Desktop, real render check

This is not a Claude Code session prompt on its own. This is the
checklist a human executes, on their own machine, once a real `.mcpb`
exists. Nothing here can be delegated to a sandboxed session.

## Before you start

Confirm a real build exists: check the Actions tab for
`mcpb-release.yml`'s latest run, green not red, and that it post-dates
`bfe4d7bb` (the `timeout` → `perl -e 'alarm N; exec @ARGV'` fix). Download
the `.mcpb` artifact from that run.

## Steps

1. **Install.** Drag the `.mcpb` onto Claude Desktop, or double-click it.
   Review the install dialog like a normal user would, not like a
   developer who already trusts the source.
2. **Confirm the tools are visible.** All eight tools should show up,
   including `lopi_get_stack_status` (new since `MCPB-App-1`), wherever
   Claude Desktop lists connected MCP tools.
3. **Trigger the widget.** Submit a real lopi task if nothing's running,
   then ask Claude to check on it. Watch what happens.
4. **Pass looks like:** an actual rendered panel, not a text block,
   showing real task/branch/stage data. Cross-check against `lopi dock`
   for the same task to confirm the data is correct, not just that
   something rendered.
5. **Fail looks like:** the tool call completes, Claude describes the
   result in text, no panel appears, silent, no error. Capture Claude
   Desktop's version, any developer/MCP logs it exposes, the exact prompt
   used, and whether step 2's tool list was correct, that narrows down
   whether the failure is tool discovery or specifically the UI handshake.

## Either way

Write the `LEDGER.md` entry. Pass: log the Claude Desktop version tested
against. Fail: log everything from step 5, a real negative result is a
complete, useful outcome here, not a failure to route around.
