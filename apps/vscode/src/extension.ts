// Dashboard for Coding Agents: a status bar item, a sidebar tree, and the dashboard in a view in the
// panel and in an editor tab, all fed by `agent-dashboard serve`, which reads Claude Code's
// transcripts into the history every surface shares. The extension starts once VS Code has, so that
// new activity is read as VS Code opens.

import type { Show } from "@agent-dashboard/protocol";
import * as vscode from "vscode";

import { Channels } from "./channel";
import { Dashboard, PANEL_TYPE } from "./panel";
import { Server, type ServerState } from "./server";
import { StatusBar } from "./statusBar";
import { SummaryModel } from "./summary";
import { SummaryTree } from "./tree";
import { DashboardView, VIEW_ID } from "./view";

/** What the tree's welcome content and the walkthrough go by. */
function setContext(key: string, value: string | boolean): void {
  void vscode.commands.executeCommand("setContext", `agentDashboard.${key}`, value);
}

export function activate(context: vscode.ExtensionContext): void {
  const log = vscode.window.createOutputChannel("Dashboard for Coding Agents", { log: true });
  const server = new Server(context.extensionPath, log);
  const summary = new SummaryModel(server, log);
  const statusBar = new StatusBar();
  const tree = new SummaryTree(summary, context.extensionUri);
  const channels = new Channels(server, summary);
  const dashboard = new Dashboard(context.extensionUri, channels);
  const view = new DashboardView(context.extensionUri, channels);

  const shown = (state: ServerState) => {
    setContext("state", state.kind === "running" && !summary.summary ? "starting" : state.kind);
    statusBar.showState(state);
    if (state.kind === "stopped") {
      void vscode.window
        .showErrorMessage(
          `${state.reason}. The dashboard cannot show your usage.`,
          "Restart",
          "Show Log",
        )
        .then((chosen) => {
          if (chosen === "Restart") void server.restart();
          else if (chosen === "Show Log") log.show();
        });
    }
  };
  shown(server.state);

  context.subscriptions.push(
    log,
    server,
    summary,
    statusBar,
    tree,
    channels,
    dashboard,
    view,
    vscode.window.createTreeView("agentDashboard.summary", { treeDataProvider: tree }),
    vscode.window.registerWebviewPanelSerializer(PANEL_TYPE, dashboard),
    vscode.window.registerWebviewViewProvider(VIEW_ID, view),
    server.onDidChangeState(shown),
    summary.onDidChange(() => {
      const current = summary.summary;
      // Once the server has stopped, the status bar says so rather than show its last numbers.
      if (!current || server.state.kind !== "running") return;
      statusBar.show(current);
      setContext("state", "running");
      setContext("limitsConnected", current.limitsConnected);
      setContext("importOffered", summary.importOffered);
    }),
    vscode.commands.registerCommand("agentDashboard.open", () => {
      dashboard.open();
    }),
    vscode.commands.registerCommand("agentDashboard.openInEditor", () => {
      dashboard.open();
    }),
    vscode.commands.registerCommand("agentDashboard.show", (show: Show) => {
      dashboard.open(show);
    }),
    vscode.commands.registerCommand("agentDashboard.refresh", () =>
      server.request("refresh", null).catch((error: unknown) => {
        log.error(`Cannot read new activity: ${String(error)}`);
      }),
    ),
    vscode.commands.registerCommand("agentDashboard.connectLimits", () => {
      dashboard.open({ section: "limits" });
    }),
    vscode.commands.registerCommand("agentDashboard.importHistory", () => {
      dashboard.open({ section: "settings" });
    }),
    vscode.commands.registerCommand("agentDashboard.chooseFolders", () => {
      dashboard.open({ section: "settings" });
    }),
    vscode.commands.registerCommand("agentDashboard.showLog", () => {
      log.show();
    }),
    vscode.commands.registerCommand("agentDashboard.restart", () => server.restart()),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("agentDashboard.server.path")) void server.restart();
    }),
  );
  void server.start();
}
