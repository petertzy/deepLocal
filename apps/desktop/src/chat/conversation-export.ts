export type ExportChatMessage = {
  id?: string;
  role: "user" | "assistant";
  content: string;
  created_at?: string;
};

export type ExportChatConversation = {
  id: string;
  title: string;
  model_id?: string | null;
  messages: ExportChatMessage[];
  created_at: string;
  updated_at: string;
};

/**
 * JSON schema:
 * {
 *   "schema": "deeplocal.conversation-export",
 *   "schema_version": 1,
 *   "conversation": {
 *     "id": string,
 *     "title": string,
 *     "model_id": string | null,
 *     "created_at": string,
 *     "updated_at": string,
 *     "messages": [{ "id"?: string, "role": "user" | "assistant",
 *                    "content": string, "created_at"?: string }]
 *   }
 * }
 */
export function serializeConversationJson(conversation: ExportChatConversation): string {
  return JSON.stringify(
    {
      schema: "deeplocal.conversation-export",
      schema_version: 1,
      conversation: {
        id: conversation.id,
        title: conversation.title,
        model_id: conversation.model_id ?? null,
        created_at: conversation.created_at,
        updated_at: conversation.updated_at,
        messages: conversation.messages.map((message) => ({
          ...(message.id === undefined ? {} : { id: message.id }),
          role: message.role,
          content: message.content,
          ...(message.created_at === undefined ? {} : { created_at: message.created_at }),
        })),
      },
    },
    null,
    2,
  );
}

export function serializeConversationMarkdown(conversation: ExportChatConversation): string {
  const lines = [`# ${conversation.title}`, "", `- Created: ${conversation.created_at}`, `- Updated: ${conversation.updated_at}`, ""];

  for (const message of conversation.messages) {
    lines.push(`## ${message.role === "user" ? "User" : "Assistant"}`, "");

    if (message.created_at) {
      lines.push(`*${message.created_at}*`, "");
    }

    lines.push(message.content, "");
  }

  return `${lines.join("\n").trimEnd()}\n`;
}
