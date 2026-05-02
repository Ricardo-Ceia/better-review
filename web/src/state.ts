import type { ExplainHistoryResponse, ExplainModelsResponse, ExplainSessionsResponse, FocusTarget, Screen, SettingsResponse } from './types';

export const initialSettings: SettingsResponse = { has_github_token: false };
export const initialExplainSessions: ExplainSessionsResponse = { available: false, selected_session_id: null, sessions: [] };
export const initialExplainModels: ExplainModelsResponse = { available: false, selected_model: null, models: [] };
export const initialExplainHistory: ExplainHistoryResponse = { runs: [] };

export const initialFocus: FocusTarget = 'files';
export const initialScreen: Screen = 'home';
