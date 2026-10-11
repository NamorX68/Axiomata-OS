<!--
  An agent's status as the app's status mark (`ui/StatusMark.svelte`) — in its pane's status line, on its dock tab
  and in the agent picker (M7.2 CP6). One component, so the three places cannot drift into three colour schemes.

  `waiting` breathes: it is the one state that needs a human, and the whole point of showing it on a tab is that it is
  noticed in a pane nobody is looking at. `working` glows (the accent rule).
-->
<script lang="ts">
  import StatusMark from "../ui/StatusMark.svelte";
  import { agentTone } from "../ui/status";
  import type { StatusView } from "./agentStatus";

  let { view }: { view: StatusView } = $props();
</script>

<StatusMark tone={agentTone(view.tone)} label={view.label} title={view.title} />
