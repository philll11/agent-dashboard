# Copilot CLI's charges are recorded, not calculated

Copilot CLI records with each request what GitHub charged for it, in nano AI credits, so the history stores that charge per request and never prices Copilot CLI's requests from a table.
GitHub's figure includes what no price table reproduces, such as the 10% it adds to requests that stay in a data residency region, and it is what the person's plan counts.
This amends ADR 0004 for Copilot CLI: Claude Code's API-equivalent cost is still calculated when queried, from the price table.
ADR 0017 records Copilot in VS Code's charges the same way, as one total per prompt where VS Code's own files hold them.

Cost adds up across agents into one figure in US dollars, at $0.01 an AI credit, and every figure says where it comes from: an estimate when it holds Claude Code's API-equivalent cost alone, recorded when it holds Copilot CLI's charges alone, and mixed when it holds both.
The agent filter splits a mixed figure into its parts.

## Considered options

- **Pricing Copilot CLI's requests from a price table**, as Claude Code's are, would make every figure an estimate, but GitHub publishes rates per model that change, and leaves out the uplifts it applies; the figure would differ from the one the person's allowance counts.
- **Two figures, one per agent, never added up** would keep estimates and records apart, but leave the person to add them, and every tile, chart and export would need two of everything.
- **Counting Copilot CLI in premium requests**, its older unit, would match neither GitHub's billing nor the allowance, which count AI credits.

## Consequences

- A Copilot CLI request without a charge, such as one on the person's own key, is unpriced, never free.
- Correcting the price table changes Claude Code's figures only; a Copilot CLI charge is final once read.
- Reconciliation with a reported session cost applies to Claude Code's sessions only.
