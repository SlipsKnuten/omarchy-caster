import QtQuick
import QtTest
import "../Model.js" as Model

TestCase {
  name: "CasterModel"

  function test_envelope() {
    compare(Model.envelope('{"api":1,"ok":true,"result":{"state":"idle"}}').state, "idle")
    verifyException(function() { Model.envelope('{"api":2,"ok":true,"result":{}}') })
    verifyException(function() {
      Model.envelope('{"api":1,"ok":false,"error":{"code":"test","message":"failed"}}')
    })
  }

  function test_sourceFiltering() {
    var rows = [
      { id: "window:1", kind: "window" },
      { id: "screen:1", kind: "screen" }
    ]
    compare(Model.byKind(rows, "window").length, 1)
    compare(Model.firstId(rows, "screen:1"), "screen:1")
    compare(Model.firstId(rows, "missing"), "window:1")
  }

  function test_activeStates() {
    verify(Model.isActive("connecting"))
    verify(Model.isActive("negotiating"))
    verify(Model.isActive("streaming"))
    verify(Model.isActive("stopping"))
    verify(!Model.isActive("idle"))
    verify(!Model.isActive("error"))
  }

  function test_optionsAreStable() {
    compare(Model.qualityOptions().map(function(row) { return row.value }).join(","),
            "auto,720p,1080p")
    compare(Model.bitrateOptions().map(function(row) { return row.value }).join(","),
            "auto,4,6,8")
  }
}
