/**
 * Settings — Phase 4.1 (AI provider config) + appearance.
 *
 * Manage the Anthropic API key (stored in the OS keychain, never in plaintext),
 * pick a default model, test the connection, and review/revoke AI consent.
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, KeyRound, Loader2, Trash2 } from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { AiTestResult } from "@/lib/bindings";
import {
  hasAiConsent,
  grantAiConsent,
  revokeAiConsent,
  preferredModel,
  setPreferredModel,
} from "@/lib/aiConsent";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Input,
  Select,
  Separator,
} from "@/components/ui";
import { ThemeToggle } from "@/components/ThemeToggle";

export function SettingsPage() {
  const qc = useQueryClient();
  const [keyInput, setKeyInput] = useState("");
  const [model, setModel] = useState(preferredModel() ?? "claude-sonnet-4-6");
  const [test, setTest] = useState<AiTestResult | null>(null);
  const [consent, setConsent] = useState(hasAiConsent());

  const statusQuery = useQuery({
    queryKey: ["aiKeyStatus"],
    queryFn: () => ipc.ai.keyStatus(),
  });
  const modelsQuery = useQuery({
    queryKey: ["aiModels"],
    queryFn: () => ipc.ai.models(),
  });

  const saveKey = useMutation({
    mutationFn: () => ipc.ai.keySet(keyInput.trim()),
    onSuccess: () => {
      setKeyInput("");
      void qc.invalidateQueries({ queryKey: ["aiKeyStatus"] });
    },
  });
  const clearKey = useMutation({
    mutationFn: () => ipc.ai.keyClear(),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["aiKeyStatus"] }),
  });
  const testConn = useMutation({
    mutationFn: () => ipc.ai.testConnection(model),
    onSuccess: setTest,
  });

  const status = statusQuery.data;

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-2xl px-8 py-10">
        <header className="mb-8">
          <h1 className="text-[var(--text-ui-3xl)] font-bold">Innstillinger</h1>
        </header>

        <Card className="mb-6">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <KeyRound size={18} className="text-[var(--color-accent)]" />
              AI — Anthropic
            </CardTitle>
            <CardDescription>
              Nøkkelen lagres i systemets nøkkelring, aldri i klartekst.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center gap-2 text-sm">
              <span className="text-[var(--color-fg-muted)]">Status:</span>
              {status?.stored ? (
                <Badge variant="success">Lagret i nøkkelring</Badge>
              ) : status?.env ? (
                <Badge variant="neutral">Fra miljøvariabel</Badge>
              ) : (
                <Badge variant="warning">Ingen nøkkel</Badge>
              )}
            </div>

            <div className="flex items-center gap-2">
              <Input
                type="password"
                value={keyInput}
                onChange={(e) => setKeyInput(e.target.value)}
                placeholder="sk-ant-…"
              />
              <Button
                onClick={() => saveKey.mutate()}
                disabled={keyInput.trim().length === 0 || saveKey.isPending}
              >
                Lagre
              </Button>
              {status?.stored && (
                <Button
                  variant="outline"
                  size="icon"
                  title="Fjern lagret nøkkel"
                  onClick={() => clearKey.mutate()}
                  disabled={clearKey.isPending}
                >
                  <Trash2 size={15} />
                </Button>
              )}
            </div>

            <div className="flex items-center gap-2">
              <label className="text-sm text-[var(--color-fg-muted)]">
                Standardmodell
              </label>
              <Select
                className="max-w-xs"
                value={model}
                onChange={(e) => {
                  setModel(e.target.value);
                  setPreferredModel(e.target.value);
                }}
              >
                {(modelsQuery.data ?? []).map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.display}
                  </option>
                ))}
              </Select>
            </div>

            <div className="flex items-center gap-3">
              <Button
                variant="secondary"
                onClick={() => testConn.mutate()}
                disabled={testConn.isPending}
              >
                {testConn.isPending && (
                  <Loader2 size={14} className="animate-spin" />
                )}
                Test tilkobling
              </Button>
              {test && (
                <span
                  className={
                    test.ok
                      ? "flex items-center gap-1 text-sm text-[var(--color-success)]"
                      : "text-sm text-[var(--color-danger)]"
                  }
                >
                  {test.ok && <Check size={14} />}
                  {test.message}
                </span>
              )}
            </div>

            <p className="text-xs text-[var(--color-fg-muted)]">
              Bruksmåling per måned kommer i en senere versjon.
            </p>
          </CardContent>
        </Card>

        <Card className="mb-6">
          <CardHeader>
            <CardTitle>Personvern for AI</CardTitle>
            <CardDescription>
              AI-funksjoner er valgfrie og sender innhold til Anthropic.
            </CardDescription>
          </CardHeader>
          <CardContent className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-sm">
              <span className="text-[var(--color-fg-muted)]">Samtykke:</span>
              {consent ? (
                <Badge variant="success">Gitt</Badge>
              ) : (
                <Badge variant="neutral">Ikke gitt</Badge>
              )}
            </div>
            {consent ? (
              <Button
                variant="outline"
                onClick={() => {
                  revokeAiConsent();
                  setConsent(false);
                }}
              >
                Trekk tilbake
              </Button>
            ) : (
              <Button
                onClick={() => {
                  grantAiConsent();
                  setConsent(true);
                }}
              >
                Gi samtykke
              </Button>
            )}
          </CardContent>
        </Card>

        <Separator className="my-6" />

        <Card>
          <CardHeader>
            <CardTitle>Utseende</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between">
              <span className="text-sm text-[var(--color-fg-muted)]">Tema</span>
              <ThemeToggle />
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
