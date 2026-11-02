import SwiftUI

private struct TailPosition: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

struct ChatPane: View {
    @Bindable var model: ClientModel
    @State private var following = true
    @State private var showSkills = false
    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Label(tr("chat"), systemImage: "bubble.left.and.bubble.right").font(.callout.weight(.medium))
                Spacer()
                if let state = model.snapshot, let id = state.selectedAgent,
                   state.agents.first(where: { $0.id == id })?.canPause == true {
                    Button(tr(state.agentPause == .paused ? "resume_agent" : "pause_agent")) {
                        model.command { try await $0.setAgentPaused(agentId: id, paused: state.agentPause != .paused) }
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
                                ApprovalCard(model: model, approval: approval, agent: agent).id(approval.id)
                            }
                            if let question = model.snapshot?.question, let agent = model.snapshot?.selectedAgent {
                                QuestionCard(model: model, question: question, agent: agent).id(question.id)
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
                    .onChange(of: model.snapshot?.conversationContext) { _, _ in following = true; proxy.scrollTo("tail", anchor: .bottom) }
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
                HStack(spacing: 8) {
                    Menu {
                        ForEach(state.composer.models, id: \.model) { choice in
                            Button(choice.name) { model.command { try await $0.setChatModel(agentId: agent, model: choice.model, effort: choice.defaultEffort) } }
                        }
                        Divider()
                        Button(tr("refresh")) { model.command { try await $0.reloadComposer() } }
                    } label: { Label(state.composer.model.isEmpty ? tr("model") : state.composer.model, systemImage: "cpu") }
                        .menuStyle(.borderlessButton).fixedSize().disabled(state.composer.loading)
                    if let selected = state.composer.models.first(where: { $0.model == state.composer.model }), !selected.efforts.isEmpty {
                        Menu(state.composer.effort) {
                            ForEach(selected.efforts, id: \.self) { effort in
                                Button(effort) { model.command { try await $0.setChatModel(agentId: agent, model: state.composer.model, effort: effort) } }
                            }
                        }.menuStyle(.borderlessButton).fixedSize()
                    }
                    Spacer()
                    if let bucket = state.composer.usage.buckets.first, let window = bucket.primary {
                        Text(String(format: tr("usage_remaining"), window.remainingPercent)).font(.caption).foregroundStyle(.secondary)
                    }
                }.font(.caption)
                if !state.composer.skills.selected.isEmpty {
                    ScrollView(.horizontal) {
                        HStack {
                            ForEach(state.composer.skills.selected, id: \.path) { skill in
                                Button { model.command { try await $0.selectSkill(agentId: agent, path: skill.path, selected: false) } } label: {
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
                            Button { model.command { try await $0.removeAttachment(agentId: agent, uploadId: upload.file.id) } } label: { Image(systemName: "xmark") }.buttonStyle(.plain)
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
    let message: MessageInfo
    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            if let tool = message.tool {
                DisclosureGroup {
                    if !tool.arguments.isEmpty { Text(tool.arguments).font(.caption.monospaced()).textSelection(.enabled) }
                    if !tool.output.isEmpty { Text(tool.output).font(.caption.monospaced()).textSelection(.enabled) }
                } label: {
                    Label(tool.summary.isEmpty ? tool.label : tool.summary, systemImage: tool.failed ? "exclamationmark.circle" : "terminal")
                        .font(.callout).foregroundStyle(tool.failed ? .orange : .secondary).lineLimit(2)
                }.padding(12).background(.quaternary.opacity(0.3), in: RoundedRectangle(cornerRadius: 8))
            } else {
                Text(tr(message.kind == .user ? "you" : "agent")).font(.caption.weight(.semibold)).foregroundStyle(message.kind == .user ? Color.secondary : Color.cyan)
                Text((try? AttributedString(markdown: message.text, options: .init(interpretedSyntax: .full))) ?? AttributedString(message.text))
                    .textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading).lineSpacing(4)
            }
            ForEach(message.questionReplies, id: \.id) { reply in
                VStack(alignment: .leading, spacing: 4) {
                    Text(reply.question).foregroundStyle(.secondary); Text(reply.answer)
                }.font(.callout).padding(10).background(.quaternary, in: RoundedRectangle(cornerRadius: 6))
            }
            ForEach(message.attachments, id: \.id) { file in
                AttachmentRow(model: model, file: file, context: model.snapshot?.conversationContext ?? "")
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
            guard file.mime.hasPrefix("image/") else { return }
            model.command { core in
                let data = try await core.chatAttachment(context: context, id: file.id)
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
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(tr("approval_required"), systemImage: "hand.raised").fontWeight(.medium)
            Text(approval.prompt).textSelection(.enabled)
            if let command = approval.command { Text(command).font(.callout.monospaced()).textSelection(.enabled) }
            if let explanation = approval.explanation { Text(explanation).foregroundStyle(.secondary) }
            HStack {
                ForEach(Array(approval.choices.enumerated()), id: \.offset) { index, choice in
                    Button(choice.label) { model.command { try await $0.respondApproval(agentId: agent, requestId: approval.id, choice: UInt32(index)) } }
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
    @State private var answers: [String: String] = [:]
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Label(tr("question_required"), systemImage: "questionmark.bubble").fontWeight(.medium)
            ForEach(question.fields, id: \.id) { field in
                VStack(alignment: .leading, spacing: 8) {
                    Text(field.header).font(.caption).foregroundStyle(.secondary)
                    Text(field.text).textSelection(.enabled)
                    ForEach(field.options, id: \.label) { option in
                        Button { answers[field.id] = option.label } label: {
                            HStack(alignment: .top) {
                                Image(systemName: answers[field.id] == option.label ? "largecircle.fill.circle" : "circle")
                                VStack(alignment: .leading) {
                                    Text(option.label); if !option.description.isEmpty { Text(option.description).font(.caption).foregroundStyle(.secondary) }
                                }
                            }.frame(maxWidth: .infinity, alignment: .leading)
                        }.buttonStyle(.plain)
                    }
                    if field.options.isEmpty || field.allowOther {
                        let value = Binding(get: { answers[field.id] ?? "" }, set: { answers[field.id] = $0 })
                        if field.secret { SecureField(tr("answer"), text: value) }
                        else { TextField(tr("answer"), text: value) }
                    }
                }
            }
            Button(tr("submit_answer")) {
                let values = question.fields.map { QuestionAnswerInfo(questionId: $0.id, value: answers[$0.id] ?? "") }
                model.command { try await $0.answerQuestion(agentId: agent, requestId: question.id, answers: values) }
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
                    if let id = model.snapshot?.selectedAgent {
                        let selected = model.snapshot?.composer.skills.selected.contains { $0.path == skill.path } == true
                        model.command { try await $0.selectSkill(agentId: id, path: skill.path, selected: !selected) }
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
