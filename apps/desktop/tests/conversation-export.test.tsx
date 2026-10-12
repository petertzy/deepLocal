import { describe, expect, it } from "vitest";
import { serializeConversationJson, serializeConversationMarkdown, type ExportChatConversation } from "../src/chat/conversation-export";

const conversation: ExportChatConversation = {
  id: "conversation-1",
  title: "Test conversation",
  model_id: "fixture-model",
  created_at: "2026-10-09T10:00:00Z",
  updated_at: "2026-10-09T10:02:00Z",
  messages: [
    {
      id: "message-1",
      role: "user",
      content: "Explain an amplifier.",
      created_at: "2026-10-09T10:00:00Z",
    },
    {
      id: "message-2",
      role: "assistant",
      content: "An amplifier increases signal amplitude.",
      created_at: "2026-10-09T10:02:00Z",
    },
  ],
};

describe("conversation export serializers", () => {
  it("serializes Markdown with title, timestamps, roles, and message order", () => {
    const markdown = serializeConversationMarkdown(conversation);

    expect(markdown).toContain("# Test conversation");
    expect(markdown).toContain("- Created: 2026-10-09T10:00:00Z");
    expect(markdown).toContain("- Updated: 2026-10-09T10:02:00Z");
    expect(markdown).toContain("*2026-10-09T10:00:00Z*");
    expect(markdown.indexOf("Explain an amplifier.")).toBeLessThan(markdown.indexOf("An amplifier increases signal amplitude."));
    expect(markdown).toContain("## User");
    expect(markdown).toContain("## Assistant");
    expect(markdown.endsWith("\n")).toBe(true);
  });

  it("serializes versioned JSON with conversation metadata and ordered messages", () => {
    const json = JSON.parse(serializeConversationJson(conversation));

    expect(json.schema).toBe("deeplocal.conversation-export");
    expect(json.schema_version).toBe(1);
    expect(json.conversation.id).toBe(conversation.id);
    expect(json.conversation.title).toBe(conversation.title);
    expect(json.conversation.model_id).toBe("fixture-model");
    expect(json.conversation.created_at).toBe(conversation.created_at);
    expect(json.conversation.updated_at).toBe(conversation.updated_at);
    expect(json.conversation.messages).toEqual(conversation.messages);
  });

  it("does not modify the source conversation", () => {
    const original = JSON.stringify(conversation);

    serializeConversationJson(conversation);
    serializeConversationMarkdown(conversation);

    expect(JSON.stringify(conversation)).toBe(original);
  });

  it("preserves messages without optional IDs or timestamps", () => {
    const minimal: ExportChatConversation = {
      ...conversation,
      messages: [{ role: "assistant", content: "Hello" }],
    };

    const json = JSON.parse(serializeConversationJson(minimal));

    expect(json.conversation.messages).toEqual([{ role: "assistant", content: "Hello" }]);
    expect(serializeConversationMarkdown(minimal)).toContain("Hello");
  });

  it("preserves Unicode and multiline message content", () => {
    const content = `Hello, 世界 👋
Markdown: **bold** & <tag> "quoted"`;

    const edgeCase: ExportChatConversation = {
      ...conversation,
      messages: [{ role: "user", content }],
    };

    const json = JSON.parse(serializeConversationJson(edgeCase));
    expect(json.conversation.messages[0].content).toBe(content);

    const markdown = serializeConversationMarkdown(edgeCase);
    expect(markdown).toContain("Hello, 世界 👋");
    expect(markdown).toContain('"quoted"');
  });
});
