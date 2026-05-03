export type ReviewStatus = 'Unreviewed' | 'Accepted' | 'Rejected';
export type FileStatus = 'Added' | 'Deleted' | 'Renamed' | 'Copied' | 'ModeChanged' | 'Modified';
export type DiffLineKind = 'Add' | 'Remove' | 'Context';
export type FocusTarget = 'files' | 'hunks';
export type Screen = 'home' | 'review';

export interface ReviewCounts {
  unreviewed: number;
  accepted: number;
  rejected: number;
}

export interface DiffLine {
  kind: DiffLineKind;
  content: string;
  old_line: number | null;
  new_line: number | null;
}

export interface HunkResponse {
  header: string;
  old_start: number;
  old_count: number;
  new_start: number;
  new_count: number;
  lines: DiffLine[];
  review_status: ReviewStatus;
}

export interface FileResponse {
  old_path: string;
  new_path: string;
  display_path: string;
  display_label: string;
  status: FileStatus;
  is_binary: boolean;
  review_status: ReviewStatus;
  hunks: HunkResponse[];
}

export interface ReviewStateResponse {
  repo_path: string;
  version: number;
  counts: ReviewCounts;
  files: FileResponse[];
}

export interface SettingsResponse {
  has_github_token: boolean;
  default_explain_model: string | null;
}

export interface ActionResponse {
  message: string;
  state: ReviewStateResponse;
}

export interface ExplainSessionResponse {
  id: string;
  title: string;
  directory: string;
  time_updated: number;
}

export interface ExplainSessionsResponse {
  available: boolean;
  selected_session_id: string | null;
  sessions: ExplainSessionResponse[];
}

export interface ExplainModelsResponse {
  available: boolean;
  selected_model: string | null;
  models: string[];
}

export interface ExplainAnswerResponse {
  summary: string;
  purpose: string;
  change: string;
  risk_level: string;
  risk_reason: string;
}

export interface ExplainHistoryItem {
  id: number;
  label: string;
  model: string;
  status: 'Running' | 'Ready' | 'Failed' | string;
  answer: ExplainAnswerResponse | null;
  error: string | null;
}

export interface ExplainHistoryResponse {
  runs: ExplainHistoryItem[];
}

export interface ExplainStartResponse {
  id: number;
  label: string;
  status: string;
}

export interface WebEventPayload {
  kind: string;
  message: string;
  run_id: number | null;
}

export interface CommandItem {
  label: string;
  detail: string;
  shortcut: string;
  enabled: boolean;
  run: () => unknown | Promise<unknown>;
}
