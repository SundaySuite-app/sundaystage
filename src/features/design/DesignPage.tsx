/**
 * /design — a living style guide. Renders every design token and UI primitive
 * so changes can be eyeballed in one place. Dev-only (reached via ⌘K).
 */
import { useState, type ReactNode } from "react";

import { ThemeToggle } from "@/components/ThemeToggle";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
  Dialog,
  Input,
  Select,
  Separator,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
  Tooltip,
} from "@/components/ui";

const SEMANTIC = [
  "--color-bg",
  "--color-bg-elevated",
  "--color-bg-surface",
  "--color-fg",
  "--color-fg-muted",
  "--color-border",
  "--color-accent",
  "--color-brand",
];
const STATUS = [
  "--color-success",
  "--color-warning",
  "--color-danger",
  "--color-info",
];
const GOLD = [
  "--color-sunday-gold-300",
  "--color-sunday-gold-400",
  "--color-sunday-gold-500",
];
const UI_SCALE = ["xs", "sm", "md", "lg", "xl", "2xl", "3xl"] as const;
const STAGE_SCALE = ["sm", "md", "lg", "xl"] as const;

export function DesignPage() {
  const [tab, setTab] = useState("operator");
  const [dialogOpen, setDialogOpen] = useState(false);

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-4xl px-8 py-10">
        <header className="mb-10 flex items-start justify-between">
          <div>
            <div className="mb-1 text-xs font-medium tracking-widest text-[var(--color-accent)] uppercase">
              Designsystem
            </div>
            <h1 className="text-[var(--text-ui-3xl)] font-bold">
              SundayStage UI
            </h1>
            <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
              Levende stilguide — tokens og primitiver.
            </p>
          </div>
          <ThemeToggle />
        </header>

        <Section title="Farger — semantiske">
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            {SEMANTIC.map((v) => (
              <Swatch key={v} token={v} />
            ))}
          </div>
        </Section>

        <Section title="Farger — status">
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            {STATUS.map((v) => (
              <Swatch key={v} token={v} />
            ))}
          </div>
        </Section>

        <Section title="Aksent — Sunday gull">
          <div className="grid grid-cols-3 gap-3">
            {GOLD.map((v) => (
              <Swatch key={v} token={v} />
            ))}
          </div>
        </Section>

        <Section title="Typografi — UI-skala (Inter)">
          <div className="space-y-1">
            {UI_SCALE.map((s) => (
              <p key={s} style={{ fontSize: `var(--text-ui-${s})` }}>
                <span className="mr-3 font-mono text-xs text-[var(--color-fg-muted)]">
                  ui-{s}
                </span>
                Lovsang og forkynnelse
              </p>
            ))}
          </div>
        </Section>

        <Section title="Typografi — stage-skala (lesbar på avstand)">
          <div className="space-y-2">
            {STAGE_SCALE.map((s) => (
              <p
                key={s}
                className="leading-tight font-semibold"
                style={{ fontSize: `var(--text-stage-${s})` }}
              >
                <span className="mr-3 align-middle font-mono text-xs font-normal text-[var(--color-fg-muted)]">
                  stage-{s}
                </span>
                Amazing grace
              </p>
            ))}
          </div>
        </Section>

        <Section title="Knapper">
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="primary">Primary</Button>
            <Button variant="secondary">Secondary</Button>
            <Button variant="outline">Outline</Button>
            <Button variant="ghost">Ghost</Button>
            <Button variant="danger">Danger</Button>
            <Button disabled>Disabled</Button>
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button size="sm">Small</Button>
            <Button size="md">Medium</Button>
            <Button size="lg">Large</Button>
          </div>
        </Section>

        <Section title="Skjemakontroller">
          <div className="grid max-w-md gap-3">
            <Input placeholder="Sangtittel…" />
            <Textarea placeholder="Lyrikk…" />
            <Select defaultValue="16:9">
              <option value="16:9">16:9</option>
              <option value="4:3">4:3</option>
            </Select>
          </div>
        </Section>

        <Section title="Merker">
          <div className="flex flex-wrap gap-2">
            <Badge variant="neutral">Utkast</Badge>
            <Badge variant="accent">AI</Badge>
            <Badge variant="success">Live</Badge>
            <Badge variant="warning">Mangler nøkkel</Badge>
            <Badge variant="danger">Frakoblet</Badge>
          </div>
        </Section>

        <Section title="Kort">
          <Card className="max-w-sm">
            <CardHeader>
              <CardTitle>Velkomstgudstjeneste</CardTitle>
              <CardDescription>
                4 sanger · 1 tekstlesning · 1 kunngjøring
              </CardDescription>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-[var(--color-fg-muted)]">
                Kompilert til en cue-liste. Klar for «Gå live».
              </p>
            </CardContent>
            <CardFooter>
              <Button size="sm">Åpne</Button>
              <Button size="sm" variant="outline">
                Gå live
              </Button>
            </CardFooter>
          </Card>
        </Section>

        <Section title="Faner">
          <Tabs value={tab} onValueChange={setTab} className="space-y-3">
            <TabsList>
              <TabsTrigger value="operator">Operatør</TabsTrigger>
              <TabsTrigger value="stage">Sceneskjerm</TabsTrigger>
            </TabsList>
            <TabsContent value="operator">
              <p className="text-sm text-[var(--color-fg-muted)]">
                Cue-liste, live-preview og hurtigtaster.
              </p>
            </TabsContent>
            <TabsContent value="stage">
              <p className="text-sm text-[var(--color-fg-muted)]">
                Stor lyrikk, neste seksjon, klokke og notater.
              </p>
            </TabsContent>
          </Tabs>
        </Section>

        <Section title="Dialog + Tooltip">
          <div className="flex items-center gap-3">
            <Button onClick={() => setDialogOpen(true)}>Åpne dialog</Button>
            <Tooltip label="Forklaring som dukker opp">
              <Button variant="outline">Hold over meg</Button>
            </Tooltip>
          </div>
          <Dialog
            open={dialogOpen}
            onClose={() => setDialogOpen(false)}
            title="Avslutte live-økt?"
            description="Sceneskjermen blir svart."
            footer={
              <>
                <Button variant="ghost" onClick={() => setDialogOpen(false)}>
                  Avbryt
                </Button>
                <Button variant="danger" onClick={() => setDialogOpen(false)}>
                  Avslutt
                </Button>
              </>
            }
          />
        </Section>

        <Section title="16:9 utgangsramme">
          <div className="flex flex-wrap items-end gap-4">
            <StageFrame label="Lyrikk" selected>
              <span
                className="font-semibold"
                style={{ fontSize: "var(--text-stage-sm)" }}
              >
                Amazing grace
              </span>
            </StageFrame>
            <StageFrame label="Blackout" />
          </div>
        </Section>

        <Separator className="my-8" />
        <p className="pb-8 text-center text-xs text-[var(--color-fg-muted)]">
          Tokens i <code className="font-mono">src/styles/tokens.css</code> ·
          primitiver i <code className="font-mono">src/components/ui</code>
        </p>
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mb-10">
      <h2 className="mb-3 text-[var(--text-ui-sm)] font-semibold tracking-wide text-[var(--color-fg-muted)] uppercase">
        {title}
      </h2>
      {children}
    </section>
  );
}

function Swatch({ token }: { token: string }) {
  return (
    <div className="overflow-hidden rounded-lg border border-[var(--color-border)]">
      <div className="h-12 w-full" style={{ background: `var(${token})` }} />
      <div className="bg-[var(--color-bg-elevated)] px-2 py-1.5">
        <code className="font-mono text-[10px] text-[var(--color-fg-muted)]">
          {token.replace("--color-", "")}
        </code>
      </div>
    </div>
  );
}

function StageFrame({
  label,
  selected,
  children,
}: {
  label: string;
  selected?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-1.5">
      <div
        className="grid aspect-video w-64 place-items-center overflow-hidden rounded-md border bg-black px-4 text-center text-white"
        style={{
          borderColor: selected ? "var(--color-accent)" : "var(--color-border)",
        }}
      >
        {children}
      </div>
      <span className="text-xs text-[var(--color-fg-muted)]">{label}</span>
    </div>
  );
}
