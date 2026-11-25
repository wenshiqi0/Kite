import SwiftUI

private struct TailPosition: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

struct ChatPane: View {
    @Bindable var model: ClientModel
    @State private var following = true
    @State private var showSkills = false
    @State private var showUsage = false
    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Label(tr("chat"), systemImage: "bubble.left.and.bubble.right").font(.callout.weight(.medium))
                Spacer()
                if let state = model.snapshot, let id = state.selectedAgent,
                   state.agents.first(where: { $0.id == id })?.canPause == true {
                    Button(tr(state.agentPause == .paused ? "resume_agent" : "pause_agent")) {
                        model.command { try await $0.setAgentPaused(hostId: state.selectedHost ?? "", agentId: id, paused: state.agentPause != .paused) }
                    }.buttonStyle(.borderless).font(.caption)
                }
            }.padding(14)
            Divider()
            GeometryReader { geometry in
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 20) {
                            if model.messages.isEmpty {
                                VStack(alignment: .leading, spacing: 10) {
                                    Text(tr("chat_empty_title")).font(.title2.weight(.medium))
                                    Text(tr(model.snapshot?.selectedAgent != nil && !model.canSend ? "history_waiting" : "chat_empty_description")).foregroundStyle(.secondary)
                                }.padding(.vertical, 32)
                            }
                            ForEach(model.messages, id: \.id) { message in MessageRow(model: model, message: message) }
                            if model.snapshot?.history.loading == true { ProgressView().controlSize(.small) }
                            if model.snapshot?.history.failed == true {
                                Button(tr("retry_history")) { model.command { try await $0.retryHistory() } }
                            }
                            if let approval = model.snapshot?.approval, let agent = model.snapshot?.selectedAgent {
                                ApprovalCard(model: model, approval: approval, agent: agent, host: model.snapshot?.selectedHost ?? "").id(approval.id)
                            }
                            if let question = model.snapshot?.question, let agent = model.snapshot?.selectedAgent {
                                QuestionCard(model: model, question: question, agent: agent, host: model.snapshot?.selectedHost ?? "").id(question.id)
                            }
                            if let phase = model.snapshot?.responsePhase, phase != .idle {
                                HStack(spacing: 8) {
                                    if phase != .waiting && phase != .failed { ProgressView().controlSize(.small) }
                                    Text(phaseLabel(phase)).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                            Color.clear.frame(height: 28).id("tail")
                                .background(GeometryReader { g in Color.clear.preference(key: TailPosition.self, value: g.frame(in: .named("conversation")).maxY) })
                        }.padding(22)
                    }
                    .coordinateSpace(name: "conversation")
                    .onPreferenceChange(TailPosition.self) { y in following = y < geometry.size.height + 70 }
                    .onChange(of: model.messageRevision) { _, _ in if following { proxy.scrollTo("tail", anchor: .bottom) } }
                    .onChange(of: model.snapshot?.conversationContext) { _, _ in following = true; showSkills = false; showUsage = false; proxy.scrollTo("tail", anchor: .bottom) }
                    .overlay(alignment: .bottomTrailing) {
                        if !following {
                            Button { following = true; proxy.scrollTo("tail", anchor: .bottom) } label: { Label(tr("latest"), systemImage: "arrow.down") }
                                .buttonStyle(.bordered).padding(16)
                        }
                    }
                }
            }
            Divider()
            composer
        }.background(Color(nsColor: .textBackgroundColor))
    }
    private var composer: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let state = model.snapshot, let agent = state.selectedAgent {
                let settings = state.composer.settings.filter { $0.id != "service_tier" }
                if !settings.isEmpty {
                    HStack {
                        ForEach(settings, id: \.id) { setting in
                            Menu {
                                ForEach(setting.options, id: \.value) { choice in
                                    Button(choice.name) { model.command { try await $0.setChatSetting(hostId: state.selectedHost ?? "", agentId: agent, setting: setting.id, value: choice.value) } }
                                }
                            } label: {
                                Label(setting.options.first(where: { $0.value == setting.currentValue })?.name ?? setting.name, systemImage: "lock.shield")
                            }.menuStyle(.borderlessButton).fixedSize().disabled(!state.composer.ready || state.composer.loading)
                        }
                        Spacer()
                    }.font(.caption)
                }
                HStack(spacing: 8) {
                    Menu {
                        ForEach(state.composer.models, id: \.model) { choice in
                            Button(choice.name) { model.command { try await $0.setChatModel(hostId: state.selectedHost ?? "", agentId: agent, model: choice.model, effort: choice.defaultEffort) } }
                        }
                        Divider()
                        Button(tr("refresh")) { model.command { try await $0.reloadComposer() } }
                    } label: { Label(state.composer.model.isEmpty ? tr("model") : state.composer.model, systemImage: "cpu") }
                        .menuStyle(.borderlessButton).fixedSize().disabled(state.composer.loading)
                    if let selected = state.composer.models.first(where: { $0.model == state.composer.model }), !selected.efforts.isEmpty {
                        Menu(state.composer.effort) {
                            ForEach(selected.efforts, id: \.self) { effort in
                                Button(effort) { model.command { try await $0.setChatModel(hostId: state.selectedHost ?? "", agentId: agent, model: state.composer.model, effort: effort) } }
                            }
                        }.menuStyle(.borderlessButton).fixedSize()
                    }
                    if let speed = state.composer.settings.first(where: { $0.id == "service_tier" }) {
                        Menu {
                            ForEach(speed.options, id: \.value) { choice in
                                Button {
                                    model.command { try await $0.setChatSetting(hostId: state.selectedHost ?? "", agentId: agent, setting: speed.id, value: choice.value) }
                                } label: {
                                    if choice.value == speed.currentValue { Label(tr(choice.name), systemImage: "checkmark") }
                                    else { Text(tr(choice.name)) }
                                }
                            }
                        } label: {
                            Label(tr(speed.options.first(where: { $0.value == speed.currentValue })?.name ?? speed.name), systemImage: "bolt")
                        }.menuStyle(.borderlessButton).fixedSize()
                            .help(tr("Speed"))
                            .disabled(!state.composer.ready || state.composer.loading)
                    }
                    Spacer()
                    let providerId = model.selectedSession?.provider ?? state.agents.first(where: { $0.id == agent })?.agent
                    let usageSupported = state.providers.first(where: { $0.id == providerId })?.usage == true
                    if usageSupported {
                        Button {
                            showUsage = true
                            model.command { try await $0.refreshAccountUsage() }
                        } label: {
                            Label(usageSummary(state.composer.usage, model: state.composer.model), systemImage: "gauge.with.dots.needle.67percent")
                                .font(.caption)
                                .foregroundStyle(state.composer.usage.stale || state.composer.usage.status == .failed ? .orange : .secondary)
                        }.buttonStyle(.borderless)
                            .popover(isPresented: $showUsage) {
                                AccountUsagePopover(model: model, usage: state.composer.usage, provider: providerId, connected: state.connection == .connected && state.composer.ready)
                                    .frame(width: 340)
                            }
                    }
                }.font(.caption)
                if !state.composer.skills.selected.isEmpty {
                    ScrollView(.horizontal) {
                        HStack {
                            ForEach(state.composer.skills.selected, id: \.path) { skill in
                                Button { model.command { try await $0.selectSkill(hostId: state.selectedHost ?? "", agentId: agent, path: skill.path, selected: false) } } label: {
                                    Label(skill.displayName ?? skill.name, systemImage: "xmark.circle.fill")
                                }.buttonStyle(.bordered).font(.caption)
                            }
                        }
                    }
                }
                if !state.composer.uploads.isEmpty {
                    ForEach(state.composer.uploads, id: \.file.id) { upload in
                        HStack {
                            Image(systemName: upload.failed ? "exclamationmark.circle" : "paperclip")
                            Text(upload.file.name).lineLimit(1)
                            if !upload.ready && !upload.failed { ProgressView().controlSize(.small) }
                            Spacer()
                            Button { model.command { try await $0.removeAttachment(hostId: state.selectedHost ?? "", agentId: agent, uploadId: upload.file.id) } } label: { Image(systemName: "xmark") }.buttonStyle(.plain)
                        }.font(.caption)
                    }
                }
            }
            TextEditor(text: $model.draft).font(.body).scrollContentBackground(.hidden)
                .frame(minHeight: 65, maxHeight: 130).padding(4)
                .background(.quaternary.opacity(0.3), in: RoundedRectangle(cornerRadius: 8))
                .accessibilityLabel(tr("message_input"))
            HStack {
                Menu {
                    Button(tr("attach_file")) { chooseFiles() }
                        .disabled(model.snapshot?.providers.first(where: { $0.id == model.selectedSession?.provider })?.attachments != true)
                    Button(tr("choose_skills")) {
                        if let id = model.snapshot?.selectedAgent { model.command { try await $0.refreshSkills(agentId: id) } }
                        showSkills = true
                    }.disabled(model.snapshot?.providers.first(where: { $0.id == model.selectedSession?.provider })?.skills != true)
                } label: { Image(systemName: "plus.circle") }.menuStyle(.borderlessButton).fixedSize()
                Text(tr("send_shortcut")).font(.caption2).foregroundStyle(.tertiary)
                Spacer()
                Button { model.send() } label: { Label(tr("send"), systemImage: "arrow.up") }
                    .buttonStyle(.borderedProminent).keyboardShortcut(.return, modifiers: .command)
                    .disabled(!model.canSend || model.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }.padding(16)
            .popover(isPresented: $showSkills) { SkillPicker(model: model).frame(width: 360, height: 420) }
    }
    private func chooseFiles() {
        let panel = NSOpenPanel(); panel.allowsMultipleSelection = true; panel.canChooseDirectories = false
        if panel.runModal() == .OK { model.upload(panel.urls) }
    }
    private func phaseLabel(_ phase: ResponsePhase) -> String {
        switch phase {
        case .sending: tr("phase_sending")
        case .thinking: tr("phase_thinking")
        case .tool: tr("phase_tool")
        case .responding: tr("phase_responding")
        case .waiting: tr("phase_waiting")
        case .failed: tr("phase_failed")
        default: ""
        }
    }
}

struct MessageRow: View {
    let model: ClientModel
    let message: ItemInfo
    var readOnly = false
    private var skillContextOnly: Bool {
        !message.blocks.isEmpty && message.blocks.allSatisfy {
            if case .skillContext = $0 { return true }
            return false
        }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            if message.role == .error {
                Label(tr("phase_failed"), systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange)
            } else if message.role != .tool && !skillContextOnly {
                Text(tr(message.role == .user ? "you" : "agent"))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(message.role == .user ? Color.secondary : Color.cyan)
            }
            ForEach(Array(message.blocks.enumerated()), id: \.offset) { entry in
                switch entry.element {
                case .text(let text):
                    if message.role == .error {
                        Text(text).font(.callout).textSelection(.enabled)
                    } else {
                        Text((try? AttributedString(markdown: text, options: .init(interpretedSyntax: .full))) ?? AttributedString(text))
                            .textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading).lineSpacing(4)
                    }
                case .skillSelection(let names):
                    Label(names.joined(separator: " · "), systemImage: "sparkles")
                        .font(.caption).foregroundStyle(.secondary)
                case .tool(let tool):
                    DisclosureGroup {
                        if !tool.arguments.isEmpty { Text(tool.arguments).font(.caption.monospaced()).textSelection(.enabled) }
                        if !tool.output.isEmpty { Text(tool.output).font(.caption.monospaced()).textSelection(.enabled) }
                    } label: {
                        Label(tool.summary.isEmpty ? tool.label : tool.summary, systemImage: tool.failed ? "exclamationmark.circle" : "terminal")
                            .font(.callout).foregroundStyle(tool.failed ? .orange : .secondary).lineLimit(2)
                    }.padding(12).background(.quaternary.opacity(0.3), in: RoundedRectangle(cornerRadius: 8))
                case .questionReply(let reply):
                    VStack(alignment: .leading, spacing: 4) {
                        Text(reply.question).foregroundStyle(.secondary); Text(reply.answer)
                    }.font(.callout).padding(10).background(.quaternary, in: RoundedRectangle(cornerRadius: 6))
                case .attachment(let file):
                    if readOnly { Label(file.name, systemImage: "doc").font(.caption) }
                    else { AttachmentRow(model: model, file: file, context: model.snapshot?.conversationContext ?? "") }
                case .skillContext(let name, let path, let content):
                    DisclosureGroup {
                        Text(String(format: tr("skill_context_source"), path))
                            .font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                        Text((try? AttributedString(markdown: content, options: .init(interpretedSyntax: .full))) ?? AttributedString(content))
                            .textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                    } label: {
                        Label {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(tr("skill_context_title")).font(.caption).foregroundStyle(.secondary)
                                Text(name).font(.callout)
                            }
                        } icon: { Image(systemName: "book.closed") }
                    }.padding(12).background(.quaternary.opacity(0.3), in: RoundedRectangle(cornerRadius: 8))
                case .embed:
                    Text(tr("chat_reference_pending")).font(.callout).foregroundStyle(.secondary)
                }
            }
        }
    }
}

struct AttachmentRow: View {
    let model: ClientModel
    let file: AttachmentInfo
    let context: String
    @State private var image: NSImage?
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let image { Image(nsImage: image).resizable().scaledToFit().frame(maxHeight: 260) }
            Button { save() } label: { Label(file.name, systemImage: "arrow.down.doc") }.buttonStyle(.link)
        }.task(id: context + file.id) {
            image = nil
            guard file.mime.hasPrefix("image/") else { return }
            model.command { core in
                let data = try await core.chatAttachment(context: context, id: file.id)
                guard model.snapshot?.conversationContext == context else { return }
                image = NSImage(data: data)
            }
        }
    }
    private func save() {
        let panel = NSSavePanel(); panel.nameFieldStringValue = file.name
        guard panel.runModal() == .OK, let url = panel.url else { return }
        model.command { core in
            let data = try await core.chatAttachment(context: context, id: file.id)
            try data.write(to: url, options: .atomic)
        }
    }
}

struct ApprovalCard: View {
    let model: ClientModel
    let approval: ApprovalInfo
    let agent: String
    let host: String
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(tr("approval_required"), systemImage: "hand.raised").fontWeight(.medium)
            Text(approval.prompt).textSelection(.enabled)
            if let command = approval.command { Text(command).font(.callout.monospaced()).textSelection(.enabled) }
            if let explanation = approval.explanation { Text(explanation).foregroundStyle(.secondary) }
            if let operation = approval.operation { Text(operation).font(.caption).foregroundStyle(.secondary) }
            ForEach(Array(approval.details.enumerated()), id: \.offset) { _, detail in
                VStack(alignment: .leading, spacing: 4) {
                    Text(detail.name).font(.caption).foregroundStyle(.secondary)
                    Text(detail.value).font(.callout.monospaced()).textSelection(.enabled)
                }
            }
            HStack {
                ForEach(Array(approval.choices.enumerated()), id: \.offset) { index, choice in
                    Button(choice.label) { model.command { try await $0.respondApproval(hostId: host, agentId: agent, requestId: approval.id, choice: UInt32(index)) } }
                }
            }
        }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
            .background(.orange.opacity(0.09), in: RoundedRectangle(cornerRadius: 10))
    }
}

struct QuestionCard: View {
    let model: ClientModel
    let question: QuestionInfo
    let agent: String
    let host: String
    @State private var answers: [String: String] = [:]
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Label(tr("question_required"), systemImage: "questionmark.bubble").fontWeight(.medium)
            ForEach(question.fields, id: \.id) { field in
                VStack(alignment: .leading, spacing: 8) {
                    Text(field.header).font(.caption).foregroundStyle(.secondary)
                    Text(field.text).textSelection(.enabled)
                    let optionLabels = Set(field.options.map(\.label))
                    let parts = (answers[field.id] ?? "").split(separator: "\n", omittingEmptySubsequences: true).map(String.init)
                    let selectedOptions = field.multiSelect ? parts.filter { optionLabels.contains($0) } : (optionLabels.contains(answers[field.id] ?? "") ? [answers[field.id] ?? ""] : [])
                    let customParts = field.multiSelect ? parts.filter { !optionLabels.contains($0) } : ((answers[field.id] ?? "").isEmpty || optionLabels.contains(answers[field.id] ?? "") ? [] : [answers[field.id] ?? ""])
                    ForEach(field.options, id: \.label) { option in
                        let isSelected = field.multiSelect ? selectedOptions.contains(option.label) : answers[field.id] == option.label
                        Button {
                            if field.multiSelect {
                                var next = selectedOptions
                                if let idx = next.firstIndex(of: option.label) { next.remove(at: idx) } else { next.append(option.label) }
                                answers[field.id] = (next + customParts).joined(separator: "\n")
                            } else {
                                answers[field.id] = option.label
                            }
                        } label: {
                            HStack(alignment: .top) {
                                Image(systemName: field.multiSelect ? (isSelected ? "checkmark.square.fill" : "square") : (isSelected ? "largecircle.fill.circle" : "circle"))
                                VStack(alignment: .leading) {
                                    Text(option.label); if !option.description.isEmpty { Text(option.description).font(.caption).foregroundStyle(.secondary) }
                                }
                            }.frame(maxWidth: .infinity, alignment: .leading)
                        }.buttonStyle(.plain)
                    }
                    if field.options.isEmpty || field.allowOther {
                        let value = Binding(
                            get: { field.multiSelect ? customParts.joined(separator: "\n") : (optionLabels.contains(answers[field.id] ?? "") ? "" : (answers[field.id] ?? "")) },
                            set: { input in
                                if field.multiSelect {
                                    answers[field.id] = (input.isEmpty ? selectedOptions : selectedOptions + [input]).joined(separator: "\n")
                                } else {
                                    answers[field.id] = input
                                }
                            }
                        )
                        if field.secret { SecureField(tr("answer"), text: value) }
                        else { TextField(tr("answer"), text: value) }
                    }
                }
            }
            Button(tr("submit_answer")) {
                let values = question.fields.map { QuestionAnswerInfo(questionId: $0.id, value: answers[$0.id] ?? "") }
                model.command { try await $0.answerQuestion(hostId: host, agentId: agent, requestId: question.id, answers: values) }
            }.buttonStyle(.borderedProminent).disabled(question.fields.contains { answers[$0.id]?.isEmpty != false })
        }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
            .background(.cyan.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
    }
}

struct SkillPicker: View {
    let model: ClientModel
    @State private var search = ""
    var body: some View {
        VStack {
            TextField(tr("search_skills"), text: $search).textFieldStyle(.roundedBorder).padding()
            if model.snapshot?.composer.skills.loading == true { ProgressView() }
            List((model.snapshot?.composer.skills.available ?? []).filter { search.isEmpty || ($0.displayName ?? $0.name).localizedCaseInsensitiveContains(search) || $0.description.localizedCaseInsensitiveContains(search) }, id: \.path) { skill in
                Button {
                    if let id = model.snapshot?.selectedAgent, let host = model.snapshot?.selectedHost {
                        let selected = model.snapshot?.composer.skills.selected.contains { $0.path == skill.path } == true
                        model.command { try await $0.selectSkill(hostId: host, agentId: id, path: skill.path, selected: !selected) }
                    }
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(skill.displayName ?? skill.name); Text(skill.shortDescription ?? skill.description).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                        }
                        Spacer()
                        if model.snapshot?.composer.skills.selected.contains(where: { $0.path == skill.path }) == true { Image(systemName: "checkmark") }
                    }
                }.buttonStyle(.plain)
            }
        }
    }
}

private func usageWindowLabel(_ minutes: UInt64?) -> String {
    guard let minutes else { return tr("usage_window") }
    if minutes > 0 && minutes % 1440 == 0 { return String(format: tr("usage_days"), minutes / 1440) }
    if minutes > 0 && minutes % 60 == 0 { return String(format: tr("usage_hours"), minutes / 60) }
    return String(format: tr("usage_minutes"), minutes)
}

private func primaryUsageBucket(_ buckets: [UsageBucketInfo], model: String) -> UsageBucketInfo? {
    if let codex = buckets.first(where: { $0.id == "codex" }) { return codex }
    let lower = model.lowercased()
    if !lower.isEmpty {
        if lower.contains("argon"), let match = buckets.first(where: { $0.id.lowercased().contains("argon") || ($0.name?.lowercased().contains("argon") == true) }) {
            return match
        }
        if lower.contains("claude") || lower.contains("gpt"), let match = buckets.first(where: { $0.id == "3p" || ($0.name?.lowercased().contains("claude") == true) || ($0.name?.lowercased().contains("gpt") == true) }) {
            return match
        }
        if lower.contains("gemini") || lower.contains("orchestrator"), let match = buckets.first(where: { $0.id == "gemini" || (($0.name?.lowercased().contains("gemini") == true) && !($0.name?.lowercased().contains("argon") == true)) }) {
            return match
        }
    }
    return buckets.first
}

private func usageSummary(_ usage: AccountUsageInfo, model: String) -> String {
    if usage.status == .notLoaded { return tr(usage.loading ? "usage_loading" : "usage_open") }
    if usage.status == .failed || usage.stale { return tr("usage_stale_summary") }
    if usage.status == .unavailable { return tr("usage_not_provided") }
    if let bucket = primaryUsageBucket(usage.buckets, model: model) {
        let windows = [bucket.primary, bucket.secondary].compactMap { $0 }
        if !windows.isEmpty {
            return windows.map { String(format: tr("usage_short_remaining"), usageWindowLabel($0.durationMins), $0.remainingPercent) }.joined(separator: " · ")
        }
    }
    if let plan = usage.plan, !plan.isEmpty { return plan }
    return String(format: tr("usage_groups"), usage.buckets.count)
}

private struct AccountUsagePopover: View {
    let model: ClientModel
    let usage: AccountUsageInfo
    let provider: String?
    let connected: Bool
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(tr("usage_title")).font(.headline)
                if let plan = usage.plan, !plan.isEmpty {
                    Text(plan.capitalized).font(.caption).foregroundStyle(.secondary)
                } else if provider == "antigravity" {
                    Text("Antigravity").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                if usage.loading { ProgressView().controlSize(.small) }
                Button(tr("usage_refresh")) {
                    model.command { try await $0.refreshAccountUsage() }
                }.buttonStyle(.bordered).controlSize(.small).disabled(!connected || usage.loading)
            }
            ScrollView { AccountUsageDetails(usage: usage, provider: provider) }
                .frame(maxHeight: 360)
        }.padding(14)
    }
}

private struct AccountUsageDetails: View {
    let usage: AccountUsageInfo
    let provider: String?
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if usage.status == .failed || usage.stale {
                Text(tr(usage.updatedMs == nil ? "usage_failed" : "usage_stale"))
                    .font(.caption).foregroundStyle(.orange)
            }
            if usage.status == .unavailable {
                Text(tr(unavailableKey)).font(.caption).foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 10) {
                ForEach(Array(usage.buckets.enumerated()), id: \.offset) { _, bucket in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(bucket.name?.isEmpty == false ? bucket.name! : (bucket.id.isEmpty ? (provider == "antigravity" ? "Antigravity" : "Codex") : bucket.id))
                            .font(.subheadline.weight(.medium))
                        ForEach(Array([bucket.primary, bucket.secondary].compactMap { $0 }.enumerated()), id: \.offset) { _, window in
                            let remaining = min(window.remainingPercent, 100)
                            let used = 100 - remaining
                            VStack(alignment: .leading, spacing: 3) {
                                Text(String(format: tr(usage.stale ? "usage_window_previous" : "usage_window_detail"), usageWindowLabel(window.durationMins), remaining, used))
                                    .font(.caption)
                                ProgressView(value: Double(remaining), total: 100)
                                    .tint(usage.stale ? .secondary : .accentColor)
                                if let resetsAt = window.resetsAt, resetsAt > 0 {
                                    let date = Date(timeIntervalSince1970: TimeInterval(resetsAt))
                                    Text(String(format: tr("usage_resets"), date.formatted(date: .abbreviated, time: .shortened)))
                                        .font(.caption2).foregroundStyle(.secondary)
                                }
                            }
                        }
                        if bucket.limitReached {
                            Text(tr("usage_limit_reached")).font(.caption).foregroundStyle(.orange)
                        }
                        if bucket.spendControlReached {
                            Text(tr("usage_spend_limit")).font(.caption).foregroundStyle(.orange)
                        }
                    }
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 8))
                }
            }
            if let credits = usage.resetCredits {
                Text(String(format: tr("usage_reset_cards"), credits.availableCount))
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let updatedMs = usage.updatedMs {
                let date = Date(timeIntervalSince1970: TimeInterval(updatedMs) / 1000)
                Text(String(format: tr("usage_updated"), date.formatted(date: .omitted, time: .shortened)))
                    .font(.caption2).foregroundStyle(.secondary)
            }
        }
    }
    private var unavailableKey: String {
        switch usage.unavailable {
        case .noAccount: return "usage_no_account"
        case .unsupportedAuth: return "usage_auth_unavailable"
        default: return "usage_not_provided"
        }
    }
}

struct WorkspaceAccountUsageSheet: View {
    let model: ClientModel
    @State private var hosts: [HostAccountUsageInfo]?
    @State private var loading = false
    @State private var failed = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text(tr("usage_overview")).font(.headline)
                Spacer()
                if loading { ProgressView().controlSize(.small).accessibilityLabel(tr("usage_loading")) }
                Button(tr("usage_refresh")) { Task { await refresh() } }.disabled(loading)
                Button(tr("usage_close")) { dismiss() }.keyboardShortcut(.cancelAction)
            }
            if failed { Text(tr("usage_failed")).foregroundStyle(.orange) }
            if hosts?.isEmpty == true { Text(tr("usage_no_hosts")).foregroundStyle(.secondary) }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    ForEach(hosts ?? [], id: \.hostId) { host in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(host.host).font(.headline)
                            switch host.status {
                            case .offline:
                                Text(tr("usage_host_offline")).foregroundStyle(.secondary)
                            case .unavailable:
                                Text(tr("usage_host_unavailable")).foregroundStyle(.secondary)
                            case .ready:
                                if host.providers.isEmpty {
                                    Text(tr("usage_host_no_providers")).foregroundStyle(.secondary)
                                }
                                ForEach(host.providers, id: \.provider) { profile in
                                    HStack {
                                        Text(profile.provider == "codex" ? "Codex" : providerLabel(profile.provider))
                                            .fontWeight(.medium)
                                        if let plan = profile.usage.plan, !plan.isEmpty {
                                            Text(plan.capitalized).foregroundStyle(.secondary)
                                        }
                                    }.font(.subheadline)
                                    AccountUsageDetails(usage: profile.usage, provider: profile.provider)
                                }
                            }
                        }.frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
        .padding(20).frame(width: 540, height: 480)
        .task { await refresh() }
    }

    private func refresh() async {
        guard !loading else { return }
        loading = true
        failed = false
        defer { loading = false }
        do {
            hosts = try await model.workspaceAccountUsage()
        } catch is CancellationError {
            return
        } catch {
            hosts = nil
            failed = true
        }
    }
}
