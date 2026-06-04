// Barrel for the SundayStage UI primitives. Built on Tailwind v4 design
// tokens (see src/styles/tokens.css). Theming flows through semantic CSS
// variables, so primitives never hard-code light/dark colors.
//
// Shipped in Phase 0.3: Button, Input, Textarea, Select, Badge, Card,
// Separator, Tabs, Dialog, Tooltip.
// Deferred until a feature needs them: Combobox, Sheet, Popover, Toast,
// DataTable (DataTable lands with the virtualized library in Phase 2.2).

export { Button, type ButtonProps } from "./button";
export { Input } from "./input";
export { Textarea } from "./textarea";
export { Select } from "./select";
export { Badge, type BadgeProps } from "./badge";
export {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  CardFooter,
} from "./card";
export { Separator } from "./separator";
export { Tabs, TabsList, TabsTrigger, TabsContent } from "./tabs";
export { Dialog } from "./dialog";
export { Tooltip } from "./tooltip";
export { ConfirmModal, type ConfirmModalProps } from "./confirm-modal";
export { ErrorToast, type ErrorToastProps } from "./error-toast";
