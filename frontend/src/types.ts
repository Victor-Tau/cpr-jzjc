export interface Filter { plan: string; plans: string[]; status: string; group: string; keyword: string }
export interface Config {
  requestModel: string; keyId: string; temperature: number | null; reasoningEffort: string; maxOutputTokens: number; timeoutSeconds: number;
  degradedModels: string[]; aliases: Record<string, string>; lowConfidencePercent: number; degradedPercent: number; confirmationRounds: number;
  lowConfidenceRetries: number; retryDelaySeconds: number; autoDisable: boolean; autoMoveGroup: boolean; targetGroupId: string; retentionDays: number;
  schedule: { enabled: boolean; kind: string; intervalMinutes: number; time: string; timezone: string; scope: string; accountIds: string[]; filter: Filter };
  autoCheckUpdates: boolean; updateIntervalHours: number; mirrorPrefix: string;
}
export interface Group { id: string; name: string }
export interface Account { id: string; name: string; email: string | null; provider: string; enabled: boolean; status: string; planType: string | null; groups: { id: string; name: string }[]; supported: boolean }
export interface Model { id: string; label: string; family: string }
export interface Bank { hash: string; commit: string; builtAt: string; models: Model[] }
export interface Banks { active: Bank; previous: Bank | null; builtin: Bank; engineCommit: string; update: { checkedAt: number | null; lastAttemptAt: number | null; error: string | null; available: Bank | null; added: string[]; removed: string[] } }
export interface Progress { stage: string; stageStartedAt: number; updatedAt: number; detail: { nextAt?: number; receivedBytes?: number; outputBytes?: number; reasoningBytes?: number; requestId?: string; round?: number; attempt?: number; expectedCount?: number } }
export interface Job { progress?: Progress; nextAt?: number; roundAttempts?: number; attemptCount: number; id: string; accountId: string; accountName: string; source: string; status: string; verdict: string; message: string; createdAt: number; finishedAt: number | null; roundCount: number; sampleCount: number; consecutive: number; prediction: string | null; probability: number | null; bankHash: string | null; disableState: string; groupState: string }
export interface Report { roundAttempts?: number; progress?: Progress; id: string; accountId: string; accountName: string; status: string; verdict: string; message: string; config: Config; bankHash: string; bankCommit: string; engineCommit: string; disableState: string; disableError: string | null; groupState: string; groupError: string | null; attempts: { requestId: string | null; at: number; error: string | null; diagnostics?: { kind: string; elapsedMs: number; receivedBytes: number; outputBytes?: number; reasoningBytes?: number; partial?: boolean; budgetMs: number | null } | null }[]; rounds: { number: number; verdict: string; finishedAt: number; score: { prediction: string; probability: number; results: { model: string; label: string; family: string; probability: number }[]; families: { family: string; display_name: string; probability: number }[] } }[] }
export interface Bridge { version: number; theme: string; request(input: { method: string; path: string; query?: string; contentType?: string; body?: string }): Promise<{ status: number; contentType: string; body: ArrayBuffer }> }
declare global { interface Window { codexProxyPlugin?: Bridge } }
