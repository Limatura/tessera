import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { useSharedDraft } from "@/hooks/use-shared-draft";

import { UploadPanel } from "./-components/upload-panel";

const homeSearchSchema = z.object({
  /** Set by `public/sw.js` after the system share sheet posted to the app. */
  // An empty string reads as "nothing shared"
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod's catch, not a Promise's
  share: z.string().optional().catch(""),
});

export const Route = createFileRoute("/_site/")({
  component: HomeComponent,
  validateSearch: homeSearchSchema,
});

function HomeComponent() {
  const { share } = Route.useSearch();
  const [shared, clearShared] = useSharedDraft(share);

  // min-w-0: this is an item of __root's grid, and the default min-width:auto
  // lets content (a long line pasted into CodeMirror) push it out, taking the
  // whole card past the viewport. No items-center: a card taller than the
  // viewport (on phones) would overflow both ways and its top would be
  // unreachable. UploadPanel's outer m-auto centres it when it fits and stacks
  // it from the top, scrollable, when it does not.
  return (
    <div className="relative z-0 flex min-h-svh w-full min-w-0 px-6 py-20">
      <UploadPanel onSharedDone={clearShared} shared={shared} />
    </div>
  );
}
