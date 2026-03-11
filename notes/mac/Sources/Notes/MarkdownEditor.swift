import AppKit
import SwiftUI

// MARK: - BlobAttachment

/// NSTextAttachment subclass that carries the blob ID so we can
/// round-trip between NSAttributedString and Markdown string.
final class BlobAttachment: NSTextAttachment {
    let blobID: String

    init(blobID: String) {
        self.blobID = blobID
        super.init(data: nil, ofType: nil)
    }

    required init?(coder: NSCoder) { fatalError() }

    /// Sets the image and scales it to fit within the editor width.
    func apply(image: NSImage) {
        self.image = image
        let maxW: CGFloat = 480
        let s = image.size
        if s.width > maxW {
            let scale = maxW / s.width
            bounds = CGRect(x: 0, y: -4, width: s.width * scale, height: s.height * scale)
        } else {
            bounds = CGRect(x: 0, y: -4, width: s.width, height: s.height)
        }
    }
}

// MARK: - MarkdownTextView

/// NSTextView subclass that handles image paste and drag-and-drop.
/// Images are uploaded as blobs and stored as Markdown references in the content string.
final class MarkdownTextView: NSTextView {

    var blobStore: BlobStore?

    /// Called after any content change (user typing or programmatic image insertion).
    var afterChange: (() -> Void)?

    // MARK: Init

    override init(frame: NSRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        setup()
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        registerForDraggedTypes([.fileURL, .tiff, .png])
    }

    // MARK: - Paste

    override func paste(_ sender: Any?) {
        if let data = imageData(from: NSPasteboard.general) {
            insertBlobData(data)
        } else if let str = NSPasteboard.general.string(forType: .string) {
            // Paste as plain text — avoid polluting content with RTF/HTML attributes.
            insertText(str, replacementRange: selectedRange())
        }
    }

    // MARK: - Drag & Drop

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        hasImageContent(in: sender.draggingPasteboard) ? .copy : super.draggingEntered(sender)
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        hasImageContent(in: sender.draggingPasteboard) ? .copy : super.draggingUpdated(sender)
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        if let data = imageData(from: sender.draggingPasteboard) {
            insertBlobData(data)
            return true
        }
        return super.performDragOperation(sender)
    }

    // MARK: - Image Insertion

    func insertBlobData(_ data: Data) {
        guard let store = blobStore,
              let id = try? store.save(data),
              let image = NSImage(data: data) else { return }

        // Image is already cached locally — insert with final ID immediately.
        // Works fully offline; upload happens in background and is retried on reconnect.
        let att = BlobAttachment(blobID: id)
        att.apply(image: image)
        insertAttachment(att)

        Task { @MainActor in try? await store.upload(id: id) }
    }

    // MARK: - Attachment manipulation

    private func insertAttachment(_ att: BlobAttachment) {
        let insertPos = selectedRange().location
        textStorage?.insert(attString(for: att), at: insertPos)
        setSelectedRange(NSRange(location: insertPos + 1, length: 0))
        restoreTypingAttributes()
        applyMarkdownStyling()
        afterChange?()
    }

    func removeAttachment(id: String) {
        guard let storage = textStorage else { return }
        storage.enumerateAttribute(.attachment, in: fullRange(of: storage)) { val, range, stop in
            guard let a = val as? BlobAttachment, a.blobID == id else { return }
            storage.deleteCharacters(in: range)
            stop.pointee = true
        }
        applyMarkdownStyling()
        afterChange?()
    }

    func applyImage(_ image: NSImage, forBlobID id: String) {
        guard let storage = textStorage else { return }
        storage.enumerateAttribute(.attachment, in: fullRange(of: storage)) { val, range, _ in
            guard let a = val as? BlobAttachment, a.blobID == id else { return }
            a.apply(image: image)
            storage.edited(.editedAttributes, range: range, changeInLength: 0)
        }
    }

    // MARK: - Markdown extraction

    /// Converts current NSAttributedString back to Markdown:
    /// BlobAttachment → "![](amadeuz://blob/id)", everything else → plain text.
    func extractMarkdown() -> String {
        guard let storage = textStorage else { return "" }
        var result = ""
        storage.enumerateAttributes(in: fullRange(of: storage)) { attrs, range, _ in
            if let a = attrs[.attachment] as? BlobAttachment {
                result += "![](amadeuz://blob/\(a.blobID))"
            } else {
                result += (storage.string as NSString).substring(with: range)
            }
        }
        return result
    }

    // MARK: - Pasteboard helpers

    private func imageData(from pb: NSPasteboard) -> Data? {
        if let d = pb.data(forType: .png) { return d }
        if let d = pb.data(forType: .tiff) { return d }
        if let urls = pb.readObjects(forClasses: [NSURL.self], options: nil) as? [URL],
           let url = urls.first, url.isFileURL,
           let d = try? Data(contentsOf: url),
           NSImage(data: d) != nil { return d }
        return nil
    }

    private func hasImageContent(in pb: NSPasteboard) -> Bool {
        pb.availableType(from: [.png, .tiff, .fileURL]) != nil
    }

    /// Wraps an attachment in an attributed string that carries the correct text color,
    /// preventing the NSTextView from deriving black from a color-less attachment character.
    private func attString(for att: BlobAttachment) -> NSAttributedString {
        let s = NSMutableAttributedString(attachment: att)
        s.addAttribute(.foregroundColor, value: NSColor.labelColor,
                       range: NSRange(location: 0, length: s.length))
        return s
    }

    /// After any programmatic insert, the cursor sits next to a character whose attributes
    /// the NSTextView uses to rebuild typingAttributes. Re-inject the correct color so that
    /// text typed after an image is not black.
    private func restoreTypingAttributes() {
        var attrs = typingAttributes
        attrs[.foregroundColor] = NSColor.labelColor
        typingAttributes = attrs
    }

    // MARK: - Markdown styling

    /// Fast path: re-styles only the paragraph containing the cursor.
    /// Called on every keystroke — avoids full-document layout invalidation.
    func applyMarkdownStylingForCurrentLine() {
        guard let storage = textStorage, storage.length > 0 else { return }
        let cursor  = min(selectedRange().location, storage.length - 1)
        let nsStr   = storage.string as NSString
        let paraRange = nsStr.paragraphRange(for: NSRange(location: cursor, length: 0))
        guard paraRange.length > 0 else { return }

        var attachmentRanges: [NSRange] = []
        storage.enumerateAttribute(.attachment, in: paraRange, options: []) { att, range, _ in
            if att != nil { attachmentRanges.append(range) }
        }

        storage.beginEditing()

        let baseFont = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        storage.addAttribute(.font, value: baseFont, range: paraRange)
        storage.addAttribute(.foregroundColor, value: NSColor.labelColor, range: paraRange)
        storage.removeAttribute(.strikethroughStyle, range: paraRange)
        for range in attachmentRanges {
            storage.addAttribute(.foregroundColor, value: NSColor.labelColor, range: range)
        }

        // Strip trailing line terminator before style matching.
        var lineRange = paraRange
        let lastChar = nsStr.character(at: lineRange.location + lineRange.length - 1)
        if lastChar == 0x000A || lastChar == 0x000D || lastChar == 0x2028 || lastChar == 0x2029 {
            lineRange.length -= 1
        }
        if lineRange.length > 0 {
            applyLineStyle(nsStr.substring(with: lineRange), range: lineRange, in: storage)
        }

        storage.endEditing()
    }

    /// Full-document re-style. Use only on load and after image operations.
    func applyMarkdownStyling() {
        guard let storage = textStorage, storage.length > 0 else { return }
        let nsStr = storage.string as NSString
        let full  = NSRange(location: 0, length: storage.length)

        // Collect attachment ranges before any mutation to avoid re-entrancy issues.
        var attachmentRanges: [NSRange] = []
        storage.enumerateAttribute(.attachment, in: full, options: []) { att, range, _ in
            if att != nil { attachmentRanges.append(range) }
        }

        storage.beginEditing()

        // Reset all characters to the base style.
        let baseFont = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        storage.addAttribute(.font, value: baseFont, range: full)
        storage.addAttribute(.foregroundColor, value: NSColor.labelColor, range: full)
        storage.removeAttribute(.strikethroughStyle, range: full)

        // Restore foregroundColor on attachments (required for correct typingAttributes).
        for range in attachmentRanges {
            storage.addAttribute(.foregroundColor, value: NSColor.labelColor, range: range)
        }

        // Apply per-line styles.
        nsStr.enumerateSubstrings(in: full, options: .byLines) { [weak self] _, lineRange, _, _ in
            guard let self, lineRange.length > 0 else { return }
            self.applyLineStyle(nsStr.substring(with: lineRange), range: lineRange, in: storage)
        }

        storage.endEditing()
    }

    private func applyLineStyle(_ line: String, range: NSRange, in storage: NSTextStorage) {
        // Returns a range clamped to the line, starting at the line's origin.
        func prefix(_ n: Int) -> NSRange {
            NSRange(location: range.location, length: min(n, range.length))
        }
        // Returns the sub-range after `offset` characters from the line start.
        func from(_ offset: Int) -> NSRange {
            let len = range.length - offset
            guard len > 0 else { return NSRange(location: range.location + offset, length: 0) }
            return NSRange(location: range.location + offset, length: len)
        }

        if line.hasPrefix("### ") {
            storage.addAttribute(.font,
                value: NSFont.boldSystemFont(ofSize: NSFont.systemFontSize + 2), range: range)
            storage.addAttribute(.foregroundColor, value: NSColor.tertiaryLabelColor, range: prefix(4))

        } else if line.hasPrefix("## ") {
            storage.addAttribute(.font,
                value: NSFont.boldSystemFont(ofSize: NSFont.systemFontSize + 5), range: range)
            storage.addAttribute(.foregroundColor, value: NSColor.tertiaryLabelColor, range: prefix(3))

        } else if line.hasPrefix("# ") {
            storage.addAttribute(.font,
                value: NSFont.boldSystemFont(ofSize: NSFont.systemFontSize + 9), range: range)
            storage.addAttribute(.foregroundColor, value: NSColor.tertiaryLabelColor, range: prefix(2))

        } else if line.hasPrefix("- [x]") {
            // Checked item: dim the bullet, green checkbox, strikethrough on the text.
            storage.addAttribute(.foregroundColor, value: NSColor.tertiaryLabelColor, range: prefix(2))
            let checkLen = min(3, range.length - 2)
            if checkLen > 0 {
                storage.addAttribute(.foregroundColor, value: NSColor.systemGreen,
                    range: NSRange(location: range.location + 2, length: checkLen))
            }
            let rest = from(5)
            if rest.length > 0 {
                storage.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: rest)
                storage.addAttribute(.strikethroughStyle,
                    value: NSUnderlineStyle.single.rawValue, range: rest)
            }

        } else if line.hasPrefix("- [ ]") {
            // Unchecked item: dim the bullet, secondary color for the checkbox.
            storage.addAttribute(.foregroundColor, value: NSColor.tertiaryLabelColor, range: prefix(2))
            let checkLen = min(3, range.length - 2)
            if checkLen > 0 {
                storage.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor,
                    range: NSRange(location: range.location + 2, length: checkLen))
            }

        } else if line.hasPrefix("- ") {
            storage.addAttribute(.foregroundColor, value: NSColor.tertiaryLabelColor, range: prefix(2))
        }
    }

    private func fullRange(of storage: NSTextStorage) -> NSRange {
        NSRange(location: 0, length: storage.length)
    }
}

// MARK: - MarkdownEditor (SwiftUI wrapper)

struct MarkdownEditor: NSViewRepresentable {
    @Binding var markdown: String
    let blobStore: BlobStore

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let tv = MarkdownTextView(frame: NSRect(x: 0, y: 0, width: 500, height: 500))
        tv.isEditable = true
        tv.isSelectable = true
        tv.isRichText = true
        tv.allowsUndo = true
        tv.font = .systemFont(ofSize: NSFont.systemFontSize)
        tv.textContainerInset = CGSize(width: 8, height: 8)
        tv.isVerticallyResizable = true
        tv.isHorizontallyResizable = false
        tv.autoresizingMask = [.width]
        tv.minSize = NSSize(width: 0, height: 100)
        tv.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        tv.textContainer?.widthTracksTextView = true
        tv.textContainer?.containerSize = NSSize(width: 500, height: CGFloat.greatestFiniteMagnitude)
        tv.isAutomaticQuoteSubstitutionEnabled = false
        tv.isAutomaticDashSubstitutionEnabled = false
        tv.textColor = .labelColor
        tv.insertionPointColor = .labelColor
        tv.blobStore = blobStore
        tv.delegate = context.coordinator

        let coordinator = context.coordinator
        tv.afterChange = { [weak coordinator, weak tv] in
            guard let coord = coordinator, let tv = tv else { return }
            let md = tv.extractMarkdown()
            coord.lastMarkdown = md
            coord.parent.markdown = md
        }

        let sv = NSScrollView(frame: NSRect(x: 0, y: 0, width: 500, height: 500))
        sv.borderType = .noBorder
        sv.hasVerticalScroller = true
        sv.autohidesScrollers = true
        sv.documentView = tv
        sv.autoresizingMask = [.width, .height]

        context.coordinator.textView = tv
        context.coordinator.load(markdown, into: tv)
        return sv
    }

    func updateNSView(_ nsView: NSScrollView, context: Context) {
        guard let tv = nsView.documentView as? MarkdownTextView else { return }
        tv.blobStore = blobStore
        guard context.coordinator.lastMarkdown != markdown else { return }
        context.coordinator.load(markdown, into: tv)
    }

    // MARK: - Coordinator

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: MarkdownEditor
        weak var textView: MarkdownTextView?
        var lastMarkdown: String = ""

        init(_ parent: MarkdownEditor) { self.parent = parent }

        func textDidChange(_ notification: Notification) {
            guard let tv = textView else { return }
            let md = tv.extractMarkdown()
            lastMarkdown = md
            parent.markdown = md
            tv.applyMarkdownStylingForCurrentLine()
        }

        func load(_ md: String, into tv: MarkdownTextView) {
            lastMarkdown = md
            let attrStr = buildAttributedString(from: md, blobStore: parent.blobStore) { [weak tv] id, image in
                tv?.applyImage(image, forBlobID: id)
            }
            tv.textStorage?.setAttributedString(attrStr)
            tv.applyMarkdownStyling()
        }
    }
}

// MARK: - Markdown → NSAttributedString

/// Parses a Markdown content string and builds an NSAttributedString.
/// Segments matching `![alt](amadeuz://blob/id)` become BlobAttachment nodes.
/// All other text is plain text with the system font.
private func buildAttributedString(
    from md: String,
    blobStore: BlobStore,
    onImageLoaded: @escaping (String, NSImage) -> Void
) -> NSAttributedString {
    let result = NSMutableAttributedString()
    let defaultAttrs: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: NSFont.systemFontSize),
        .foregroundColor: NSColor.labelColor
    ]

    let pattern = #"!\[([^\]]*)\]\(amadeuz://blob/([a-f0-9\-]+)\)"#
    guard let regex = try? NSRegularExpression(pattern: pattern) else {
        return NSAttributedString(string: md, attributes: defaultAttrs)
    }

    let nsmd = md as NSString
    let fullRange = NSRange(location: 0, length: nsmd.length)
    var lastEnd = 0

    for match in regex.matches(in: md, range: fullRange) {
        let mRange = match.range

        // Append any plain text preceding this match.
        if mRange.location > lastEnd {
            let txt = nsmd.substring(with: NSRange(location: lastEnd,
                                                   length: mRange.location - lastEnd))
            result.append(NSAttributedString(string: txt, attributes: defaultAttrs))
        }

        let blobID = nsmd.substring(with: match.range(at: 2))
        let att = BlobAttachment(blobID: blobID)

        if let cached = blobStore.cachedData(for: blobID), let img = NSImage(data: cached) {
            // Already cached — show immediately.
            att.apply(image: img)
        } else {
            // Show placeholder and load asynchronously.
            att.image = NSImage(systemSymbolName: "photo", accessibilityDescription: nil)
            Task { @MainActor in
                guard let data = try? await blobStore.download(id: blobID),
                      let img = NSImage(data: data) else { return }
                onImageLoaded(blobID, img)
            }
        }

        let attStr = NSMutableAttributedString(attachment: att)
        attStr.addAttribute(.foregroundColor, value: NSColor.labelColor,
                            range: NSRange(location: 0, length: attStr.length))
        result.append(attStr)
        lastEnd = mRange.location + mRange.length
    }

    // Append any trailing plain text.
    if lastEnd < nsmd.length {
        result.append(NSAttributedString(string: nsmd.substring(from: lastEnd),
                                         attributes: defaultAttrs))
    }

    return result
}
