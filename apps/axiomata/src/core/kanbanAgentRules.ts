/**
 * What the assistant and the agents may do to the board through the Kanban shell actions (glut-round, fifth audit).
 *
 * Those actions run with the owner's app behind them, but their callers are model turns whose words may come from
 * untrusted text. So they keep to plain cards in plain open columns, as the CLI does under `AXIOMATA_HARNESS_RUN`:
 * approving a proposal, sending a card to review or finishing it, and changing a card of the agent flow stay the
 * owner's clicks in the board itself, which does not go through these actions.
 */

/** A column as `list_board_columns` returns it. */
export interface BoardColumn {
  id: number;
  name: string;
  maps_to_status: string;
  stage: string | null;
}

/** The fields of a card that make it part of the agent flow, as `list_board_cards` returns them. */
export interface FlowCard {
  id: number;
  column_id: number;
  plan_id?: number | null;
  agent?: string | null;
  claimed_by?: string | null;
  acceptance?: string | null;
}

/** Why a shell action may not put a card into `column`, or `null` when it may: only a plain open column. */
export function targetRefusal(column: BoardColumn): string | null {
  if (column.stage !== null) {
    return `${column.name} is the agent flow's column; approving and reviewing are the owner's on the board`;
  }
  if (column.maps_to_status === "done") {
    return `${column.name} is a done column; finishing a card is the owner's on the board`;
  }
  return null;
}

/** Why a shell action may not move `card` at all, or `null` when it may: a plain card in a plain column. */
export function cardRefusal(card: FlowCard, columns: BoardColumn[]): string | null {
  const from = columns.find((column) => column.id === card.column_id);
  const flowCard =
    card.plan_id != null || !!card.agent || !!card.claimed_by || !!card.acceptance?.trim() || from?.stage != null;
  return flowCard ? `card #${card.id} is part of the agent flow; the owner moves it on the board` : null;
}
