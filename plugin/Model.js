function envelope(text) {
  var value = JSON.parse(String(text || ""))
  if (value.api !== 1) throw new Error("Caster controller API mismatch")
  if (!value.ok) {
    var error = value.error || {}
    throw new Error(error.message || String(error) || "Caster command failed")
  }
  return value.result
}

function byKind(sources, kind) {
  return (sources || []).filter(function(source) { return source.kind === kind })
}

function findById(rows, id) {
  for (var i = 0; i < (rows || []).length; i++) {
    if (rows[i].id === id) return rows[i]
  }
  return null
}

function firstId(rows, preferred) {
  if (findById(rows, preferred)) return preferred
  return rows && rows.length > 0 ? rows[0].id : ""
}

function isActive(state) {
  return state === "connecting" || state === "negotiating"
      || state === "streaming" || state === "stopping"
}

function stageLabel(state) {
  switch (state) {
  case "connecting": return "Connecting"
  case "negotiating": return "Negotiating"
  case "streaming": return "Casting"
  case "stopping": return "Stopping"
  case "error": return "Needs attention"
  default: return "Ready to cast"
  }
}

function sourceDescription(kind) {
  return kind === "window"
    ? "Only the selected application window is visible on the TV."
    : "The full output is visible, including notifications and other windows."
}

function qualityOptions() {
  return [
    { value: "auto", label: "Auto (recommended)" },
    { value: "720p", label: "720p · 30 fps" },
    { value: "1080p", label: "1080p · 30 fps" }
  ]
}

function bitrateOptions() {
  return [
    { value: "auto", label: "Auto" },
    { value: "4", label: "4 Mbps" },
    { value: "6", label: "6 Mbps" },
    { value: "8", label: "8 Mbps" }
  ]
}
