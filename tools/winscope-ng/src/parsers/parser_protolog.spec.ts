/*
 * Copyright (C) 2022 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
import {TraceTypeId} from "common/trace/type_id";
import {ParserFactory} from "./parser_factory";
import {Parser} from "./parser";
import {TestUtils} from "test/test_utils";
import {LogMessage} from "../common/trace/protolog";

describe("ParserProtoLog", () => {
  let parser: Parser;

  const expectedFirstLogMessage = {
    text: "InsetsSource updateVisibility for ITYPE_IME, serverVisible: false clientVisible: false",
    time: "14m10s746ms",
    tag: "WindowManager",
    level: "DEBUG",
    at: "com/android/server/wm/InsetsSourceProvider.java",
    timestamp: Number(850746266486),
  };

  beforeAll(async () => {
    const buffer = TestUtils.getFixtureBlob("trace_ProtoLog.pb");
    const parsers = await new ParserFactory().createParsers([buffer]);
    expect(parsers.length).toEqual(1);
    parser = parsers[0];
  });

  it("has expected trace type", () => {
    expect(parser.getTraceTypeId()).toEqual(TraceTypeId.PROTO_LOG);
  });

  it("provides timestamps", () => {
    const timestamps = parser.getTimestamps();
    expect(timestamps.length)
      .toEqual(50);
    expect(timestamps.slice(0, 3))
      .toEqual([850746266486, 850746336718, 850746350430]);
  });

  it("reconstructs human-readable log message", () => {
    const actualMessage = parser.getTraceEntry(850746266486)!;

    expect(actualMessage).toBeInstanceOf(LogMessage);
    expect(Object.assign({}, actualMessage)).toEqual(expectedFirstLogMessage);
  });

  it("allows retrieving all the log messages", () => {
    const actualMessages = parser.getTraceEntries();

    expect(actualMessages.length).toEqual(50);

    actualMessages.forEach(message => {
      expect(message).toBeInstanceOf(LogMessage);
    });

    const actualFirstLogMessage = Object.assign({}, actualMessages[0]);
    expect(actualFirstLogMessage).toEqual(expectedFirstLogMessage);
  });
});
