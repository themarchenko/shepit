import Foundation

public enum PromptBuilder {
    /// Stable system prompt shared by every dictation (kept constant so the prompt cache holds).
    public static func systemPrompt(profiles: [Profile] = BuiltInProfiles.all, speakerNotes: String? = nil) -> String {
        var s = """
        You are the text formatter inside Shepit, a personal dictation app on macOS. The user speaks Ukrainian and English, often mixed in one sentence. You receive the raw speech-to-text transcript plus context about where the text will be inserted, and you return the text that should be inserted. You never answer the content, never add facts or requirements, never translate unless the transcript itself is an explicit instruction to translate.

        ## Cleanup rules (all modes except raw)

        - Remove filler words and false starts ("ем", "ну", "типу", "um", "uh", "like" as filler, repeated words).
        - Apply self-corrections: "на п'яту, ні, краще на шосту" becomes "на шосту"; "Monday, actually no, Wednesday" becomes "Wednesday".
        - Punctuate and capitalize. Split into paragraphs at topic changes. A spoken enumeration becomes a list only when it has three or more items and the profile allows lists.
        - Keep the wording and register of the speaker. Do not paraphrase for style.
        - Output language is the spoken language. Inside Ukrainian text, English product names, identifiers, file paths, commands and flags stay in Latin script with canonical spelling (handleSubmit, UserService.swift, git rebase, npm run build, Vercel, Supabase, Claude Code). Fix transliterations the recognizer produced ("хендл сабміт" is handleSubmit) using the dictionary and the on-screen context.
        - Anglicisms: the speaker says English technical terms inside Ukrainian speech and the recognizer writes them in Cyrillic ("пул реквест", "рефакторінг", "комміт", "деплой"). Write such terms in Latin with standard English spelling when they name a technical concept, action, tool or artifact (pull request, refactoring, commit, deploy, staging, production, feature flag, prompt, merge). Ukrainian-inflected forms used as ordinary words stay in Cyrillic as spoken (закомітити, задеплоїти, фіча, таска).
        - Numbers: digits for quantities, dates and money; words where the speaker clearly means a word.
        - Ukrainian typography: apostrophe ' and quotes «». Dashes are plain hyphens.
        - If the transcript is an instruction about the text itself ("напиши англійською", "make it a list"), follow it.

        ## Context

        "text_before_caret" is what the field already holds before the cursor. Use it for spelling and to continue a sentence (lowercase start, no repeated words). Never copy it into your output: the text you return is inserted at the cursor, after it, so anything taken from it appears twice. Only the transcript becomes output.

        ## Modes

        - clean: cleanup only.
        - prompt: cleanup, then shape the text as a prompt for an AI assistant: one goal sentence first, then context, then constraints or questions as separate short paragraphs or a "-" list. Only reorganize what was said. Never invent a requirement that was not spoken. Short transcripts stay short.
        - raw: transcript with punctuation only.

        ## Profiles

        The context names the profile of the target app. Apply its rules.

        """
        for p in profiles {
            s += "\n### \(p.id) (\(p.name))\n"
            s += "- Markdown: \(p.allowsMarkdown ? "allowed" : "not allowed").\n"
            for rule in p.styleRules { s += "- \(rule)\n" }
        }
        if let speakerNotes = speakerNotes?.trimmingCharacters(in: .whitespacesAndNewlines), !speakerNotes.isEmpty {
            s += "\n## Speaker\n\nUse this to tell recognizer errors from what was meant:\n\(speakerNotes)\n"
        }
        s += """

        ## Output

        Return only a JSON object: {"text": string, "language": "uk" | "en" | "mixed", "new_terms": string[]}. "new_terms" lists proper nouns, product names or identifiers whose spelling you fixed and that are not in the dictionary; never include ordinary words or commands. No commentary outside the JSON.
        """
        return s
    }

    /// Per-dictation message: context, dictionary, examples, transcript.
    public static func userMessage(_ r: FormatRequest) -> String {
        var ctx: [String: Any] = [
            "profile": r.profile.id,
            "mode": r.mode.rawValue,
            "language_hint": r.languageHint ?? "auto",
        ]
        if let v = r.context.appName { ctx["app"] = v }
        if let v = r.context.bundleID { ctx["bundle_id"] = v }
        if let v = r.context.windowTitle { ctx["window_title"] = String(v.prefix(200)) }
        if let v = r.context.url { ctx["url"] = v }
        if let v = r.context.textBeforeCaret, !v.isEmpty { ctx["text_before_caret"] = String(v.suffix(500)) }
        if let v = r.context.selectedText, !v.isEmpty { ctx["selected_text"] = String(v.prefix(2000)) }
        let ctxJSON = (try? JSONSerialization.data(withJSONObject: ctx, options: [.sortedKeys]))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "{}"

        var m = "Context:\n\(ctxJSON)\n"
        // Newest entries, the same direction as the recognizer: learned terms are appended, so a long
        // dictionary would otherwise never show its recent ones to the agent.
        if !r.dictionary.isEmpty {
            m += "\nDictionary (preferred spellings): " + r.dictionary.suffix(120).joined(separator: ", ") + "\n"
        }
        if !r.examples.isEmpty {
            m += "\nRecent accepted examples for this profile:\n"
            for (raw, final) in r.examples.prefix(5) {
                m += "Transcript: \(raw)\nFinal: \(final)\n\n"
            }
        }
        m += "\nTranscript:\n\(r.transcript)"
        return m
    }

    public static func parseReply(_ text: String) -> (text: String, language: String?, newTerms: [String])? {
        guard let obj = JSONExtraction.firstObject(in: text), let out = obj["text"] as? String else { return nil }
        let language = obj["language"] as? String
        let terms = (obj["new_terms"] as? [String]) ?? []
        return (out, language, terms)
    }
}
