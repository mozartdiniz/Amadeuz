import UIKit
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

    func apply(image: UIImage) {
        self.image = image
        let maxW: CGFloat = 320
        let s = image.size
        if s.width > maxW {
            let scale = maxW / s.width
            bounds = CGRect(x: 0, y: 0, width: s.width * scale, height: s.height * scale)
        } else {
            bounds = CGRect(x: 0, y: 0, width: s.width, height: s.height)
        }
    }
}

// MARK: - MarkdownTextView

final class MarkdownTextView: UITextView {

    var blobStore: BlobStore?
    var afterChange: (() -> Void)?

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
    }

    required init?(coder: NSCoder) { fatalError() }

    // MARK: - Key handling

    override func insertText(_ text: String) {
        if text == "\n" {
            if handleEnterKey() { return }
        }
        super.insertText(text)
    }

    /// Smart list continuation on Enter, mirroring the mac md_formatter.
    /// Returns true if the key was fully handled (suppresses default Enter).
    private func handleEnterKey() -> Bool {
        let nsStr   = textStorage.string as NSString
        let cursor  = selectedRange.location
        let lineNSRange = nsStr.lineRange(for: NSRange(location: cursor, length: 0))
        let lineText = nsStr.substring(with: NSRange(location: lineNSRange.location,
                                                     length: cursor - lineNSRange.location))
        let indent  = String(lineText.prefix(while: { $0 == " " || $0 == "\t" }))
        let trimmed = String(lineText.dropFirst(indent.count))

        let bullets: [(String, String)] = [
            ("- [ ] ", "- [ ] "), ("- [x] ", "- [ ] "), ("- [X] ", "- [ ] "),
            ("+ [ ] ", "+ [ ] "), ("+ [x] ", "+ [ ] "), ("+ [X] ", "+ [ ] "),
            ("* [ ] ", "* [ ] "), ("* [x] ", "* [ ] "), ("* [X] ", "* [ ] "),
            ("- ", "- "), ("+ ", "+ "), ("* ", "* "),
        ]
        for (marker, continuation) in bullets {
            guard trimmed.hasPrefix(marker) else { continue }
            let content = String(trimmed.dropFirst(marker.count))
            if content.trimmingCharacters(in: .whitespaces).isEmpty {
                let del = NSRange(location: lineNSRange.location,
                                  length: cursor - lineNSRange.location)
                replaceInRange(del, with: "\n")
            } else {
                replaceInRange(selectedRange, with: "\n" + indent + continuation)
            }
            return true
        }

        if let (num, sep) = detectOrderedMarker(trimmed) {
            let markerLen = "\(num)\(sep) ".count
            let content   = String(trimmed.dropFirst(markerLen))
            if content.trimmingCharacters(in: .whitespaces).isEmpty {
                let del = NSRange(location: lineNSRange.location,
                                  length: cursor - lineNSRange.location)
                replaceInRange(del, with: "\n")
            } else {
                let next = (Int(num) ?? 1) + 1
                replaceInRange(selectedRange, with: "\n" + indent + "\(next)\(sep) ")
            }
            return true
        }

        return false
    }

    private func detectOrderedMarker(_ trimmed: String) -> (num: String, sep: Character)? {
        let digits = String(trimmed.prefix(while: { $0.isNumber }))
        guard !digits.isEmpty else { return nil }
        let rest = trimmed.dropFirst(digits.count)
        guard let sep = rest.first, sep == "." || sep == ")" else { return nil }
        guard rest.dropFirst().hasPrefix(" ") else { return nil }
        return (digits, sep)
    }

    /// Replace a range using UITextInput.replace so all delegate callbacks fire correctly.
    private func replaceInRange(_ nsRange: NSRange, with text: String) {
        guard let start = position(from: beginningOfDocument, offset: nsRange.location),
              let end   = position(from: beginningOfDocument, offset: nsRange.location + nsRange.length),
              let range = textRange(from: start, to: end) else { return }
        replace(range, withText: text)
    }

    // MARK: - Paste

    override func paste(_ sender: Any?) {
        let pb = UIPasteboard.general
        if let image = pb.image, let data = image.pngData() {
            insertBlobData(data)
            return
        }
        super.paste(sender)
    }

    // MARK: - Image insertion

    func insertBlobData(_ data: Data) {
        guard let store = blobStore,
              let id    = try? store.save(data),
              let image = UIImage(data: data) else { return }
        let att = BlobAttachment(blobID: id)
        att.apply(image: image)
        insertBlobAttachment(att)
        Task { @MainActor in try? await store.upload(id: id) }
    }

    private func insertBlobAttachment(_ att: BlobAttachment) {
        let pos = selectedRange.location
        let attStr = NSMutableAttributedString(attachment: att)
        attStr.addAttribute(.foregroundColor, value: UIColor.label,
                            range: NSRange(location: 0, length: attStr.length))
        textStorage.beginEditing()
        textStorage.insert(attStr, at: pos)
        textStorage.endEditing()
        selectedRange = NSRange(location: pos + 1, length: 0)
        applyMarkdownStyling()
        afterChange?()
    }

    func removeAttachment(id: String) {
        textStorage.enumerateAttribute(.attachment, in: fullRange) { val, range, stop in
            guard let a = val as? BlobAttachment, a.blobID == id else { return }
            textStorage.deleteCharacters(in: range)
            stop.pointee = true
        }
        applyMarkdownStyling()
        afterChange?()
    }

    func applyImage(_ image: UIImage, forBlobID id: String) {
        textStorage.enumerateAttribute(.attachment, in: fullRange) { val, range, _ in
            guard let a = val as? BlobAttachment, a.blobID == id else { return }
            a.apply(image: image)
            textStorage.edited(.editedAttributes, range: range, changeInLength: 0)
        }
    }

    // MARK: - Markdown extraction

    func extractMarkdown() -> String {
        var result = ""
        textStorage.enumerateAttributes(in: fullRange) { attrs, range, _ in
            if let a = attrs[.attachment] as? BlobAttachment {
                result += "![](amadeuz://blob/\(a.blobID))"
            } else {
                result += (textStorage.string as NSString).substring(with: range)
            }
        }
        return result
    }

    // MARK: - Markdown styling

    func applyMarkdownStylingForCurrentLine() {
        guard textStorage.length > 0 else { return }
        let cursor    = min(selectedRange.location, textStorage.length - 1)
        let nsStr     = textStorage.string as NSString
        let paraRange = nsStr.paragraphRange(for: NSRange(location: cursor, length: 0))
        guard paraRange.length > 0 else { return }

        var attachmentRanges: [NSRange] = []
        textStorage.enumerateAttribute(.attachment, in: paraRange) { att, range, _ in
            if att != nil { attachmentRanges.append(range) }
        }

        textStorage.beginEditing()

        let baseFont = UIFont.systemFont(ofSize: UIFont.systemFontSize)
        textStorage.addAttribute(.font, value: baseFont, range: paraRange)
        textStorage.addAttribute(.foregroundColor, value: UIColor.label, range: paraRange)
        textStorage.removeAttribute(.strikethroughStyle, range: paraRange)

        for range in attachmentRanges {
            textStorage.addAttribute(.foregroundColor, value: UIColor.label, range: range)
        }

        var lineRange = paraRange
        let lastChar = nsStr.character(at: lineRange.location + lineRange.length - 1)
        if lastChar == 0x000A || lastChar == 0x000D || lastChar == 0x2028 || lastChar == 0x2029 {
            lineRange.length -= 1
        }
        if lineRange.length > 0 {
            applyLineStyle(nsStr.substring(with: lineRange), range: lineRange)
        }

        if paraRange.location == 0 {
            applyTitleStyle(nsStr: nsStr)
        }

        textStorage.endEditing()
    }

    func applyMarkdownStyling() {
        guard textStorage.length > 0 else { return }
        let nsStr = textStorage.string as NSString

        var attachmentRanges: [NSRange] = []
        textStorage.enumerateAttribute(.attachment, in: fullRange) { att, range, _ in
            if att != nil { attachmentRanges.append(range) }
        }

        textStorage.beginEditing()

        let baseFont = UIFont.systemFont(ofSize: UIFont.systemFontSize)
        textStorage.addAttribute(.font, value: baseFont, range: fullRange)
        textStorage.addAttribute(.foregroundColor, value: UIColor.label, range: fullRange)
        textStorage.removeAttribute(.strikethroughStyle, range: fullRange)

        for range in attachmentRanges {
            textStorage.addAttribute(.foregroundColor, value: UIColor.label, range: range)
        }

        nsStr.enumerateSubstrings(in: fullRange, options: .byLines) { [weak self] _, lineRange, _, _ in
            guard let self, lineRange.length > 0 else { return }
            self.applyLineStyle(nsStr.substring(with: lineRange), range: lineRange)
        }

        applyTitleStyle(nsStr: nsStr)

        textStorage.endEditing()
    }

    /// Always styles the first line as a large bold title.
    private func applyTitleStyle(nsStr: NSString) {
        let firstNL  = nsStr.range(of: "\n")
        let titleEnd = firstNL.location != NSNotFound ? firstNL.location : textStorage.length
        guard titleEnd > 0 else { return }
        let range = NSRange(location: 0, length: titleEnd)
        textStorage.addAttribute(.font, value: UIFont.boldSystemFont(ofSize: 22), range: range)
        textStorage.addAttribute(.foregroundColor, value: UIColor.label, range: range)
        textStorage.removeAttribute(.strikethroughStyle, range: range)
    }

    private func applyLineStyle(_ line: String, range: NSRange) {
        func prefix(_ n: Int) -> NSRange {
            NSRange(location: range.location, length: min(n, range.length))
        }
        func from(_ offset: Int) -> NSRange {
            let len = range.length - offset
            guard len > 0 else { return NSRange(location: range.location + offset, length: 0) }
            return NSRange(location: range.location + offset, length: len)
        }

        if line.hasPrefix("### ") {
            textStorage.addAttribute(.font,
                value: UIFont.boldSystemFont(ofSize: UIFont.systemFontSize + 2), range: range)
            textStorage.addAttribute(.foregroundColor, value: UIColor.tertiaryLabel, range: prefix(4))

        } else if line.hasPrefix("## ") {
            textStorage.addAttribute(.font,
                value: UIFont.boldSystemFont(ofSize: UIFont.systemFontSize + 5), range: range)
            textStorage.addAttribute(.foregroundColor, value: UIColor.tertiaryLabel, range: prefix(3))

        } else if line.hasPrefix("# ") {
            textStorage.addAttribute(.font,
                value: UIFont.boldSystemFont(ofSize: UIFont.systemFontSize + 9), range: range)
            textStorage.addAttribute(.foregroundColor, value: UIColor.tertiaryLabel, range: prefix(2))

        } else if line.hasPrefix("- [x]") || line.hasPrefix("- [X]") {
            textStorage.addAttribute(.foregroundColor, value: UIColor.tertiaryLabel, range: prefix(2))
            let checkLen = min(3, range.length - 2)
            if checkLen > 0 {
                textStorage.addAttribute(.foregroundColor, value: UIColor.systemGreen,
                    range: NSRange(location: range.location + 2, length: checkLen))
            }
            let rest = from(5)
            if rest.length > 0 {
                textStorage.addAttribute(.foregroundColor, value: UIColor.secondaryLabel, range: rest)
                textStorage.addAttribute(.strikethroughStyle,
                    value: NSUnderlineStyle.single.rawValue, range: rest)
            }

        } else if line.hasPrefix("- [ ]") {
            textStorage.addAttribute(.foregroundColor, value: UIColor.tertiaryLabel, range: prefix(2))
            let checkLen = min(3, range.length - 2)
            if checkLen > 0 {
                textStorage.addAttribute(.foregroundColor, value: UIColor.secondaryLabel,
                    range: NSRange(location: range.location + 2, length: checkLen))
            }

        } else if line.hasPrefix("- ") {
            textStorage.addAttribute(.foregroundColor, value: UIColor.tertiaryLabel, range: prefix(2))
        }
    }

    private var fullRange: NSRange {
        NSRange(location: 0, length: textStorage.length)
    }
}

// MARK: - MarkdownEditor (SwiftUI wrapper)

struct MarkdownEditor: UIViewRepresentable {
    @Binding var markdown: String
    let blobStore: BlobStore
    @Binding var pendingImageData: Data?

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> UITextView {
        let tv = MarkdownTextView(frame: .zero, textContainer: nil)
        tv.isEditable             = true
        tv.isSelectable           = true
        tv.isScrollEnabled        = true
        tv.font                   = .systemFont(ofSize: UIFont.systemFontSize)
        tv.textContainerInset     = UIEdgeInsets(top: 12, left: 12, bottom: 12, right: 12)
        tv.autocorrectionType     = .no
        tv.smartQuotesType        = .no
        tv.smartDashesType        = .no
        tv.allowsEditingTextAttributes = false
        tv.textColor              = .label
        tv.backgroundColor        = .systemBackground
        tv.blobStore              = blobStore
        tv.delegate               = context.coordinator

        let coordinator = context.coordinator
        tv.afterChange = { [weak coordinator, weak tv] in
            guard let coord = coordinator, let tv = tv else { return }
            let md = tv.extractMarkdown()
            coord.lastMarkdown = md
            coord.parent.markdown = md
        }

        context.coordinator.textView = tv
        context.coordinator.load(markdown, into: tv)
        return tv
    }

    func updateUIView(_ uiView: UITextView, context: Context) {
        guard let tv = uiView as? MarkdownTextView else { return }
        tv.blobStore = blobStore

        if let data = pendingImageData {
            DispatchQueue.main.async {
                tv.insertBlobData(data)
                pendingImageData = nil
            }
        }

        guard context.coordinator.lastMarkdown != markdown else { return }
        context.coordinator.load(markdown, into: tv)
    }

    // MARK: - Coordinator

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownEditor
        weak var textView: MarkdownTextView?
        var lastMarkdown: String = ""

        init(_ parent: MarkdownEditor) { self.parent = parent }

        func textViewDidChange(_ textView: UITextView) {
            guard let tv = textView as? MarkdownTextView else { return }
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
            tv.textStorage.setAttributedString(attrStr)
            tv.applyMarkdownStyling()
        }
    }
}

// MARK: - Markdown → NSAttributedString

private func buildAttributedString(
    from md: String,
    blobStore: BlobStore,
    onImageLoaded: @escaping (String, UIImage) -> Void
) -> NSAttributedString {
    let result = NSMutableAttributedString()
    let defaultAttrs: [NSAttributedString.Key: Any] = [
        .font: UIFont.systemFont(ofSize: UIFont.systemFontSize),
        .foregroundColor: UIColor.label
    ]

    let pattern = #"!\[([^\]]*)\]\(amadeuz://blob/([a-f0-9\-]+)\)"#
    guard let regex = try? NSRegularExpression(pattern: pattern) else {
        return NSAttributedString(string: md, attributes: defaultAttrs)
    }

    let nsmd      = md as NSString
    let fullRange = NSRange(location: 0, length: nsmd.length)
    var lastEnd   = 0

    for match in regex.matches(in: md, range: fullRange) {
        let mRange = match.range

        if mRange.location > lastEnd {
            let txt = nsmd.substring(with: NSRange(location: lastEnd,
                                                   length: mRange.location - lastEnd))
            result.append(NSAttributedString(string: txt, attributes: defaultAttrs))
        }

        let blobID = nsmd.substring(with: match.range(at: 2))
        let att    = BlobAttachment(blobID: blobID)

        if let cached = blobStore.cachedData(for: blobID), let img = UIImage(data: cached) {
            att.apply(image: img)
        } else {
            att.image = UIImage(systemName: "photo")
            Task { @MainActor in
                guard let data = try? await blobStore.download(id: blobID),
                      let img  = UIImage(data: data) else { return }
                onImageLoaded(blobID, img)
            }
        }

        let attStr = NSMutableAttributedString(attachment: att)
        attStr.addAttribute(.foregroundColor, value: UIColor.label,
                            range: NSRange(location: 0, length: attStr.length))
        result.append(attStr)
        lastEnd = mRange.location + mRange.length
    }

    if lastEnd < nsmd.length {
        result.append(NSAttributedString(string: nsmd.substring(from: lastEnd),
                                         attributes: defaultAttrs))
    }

    return result
}
