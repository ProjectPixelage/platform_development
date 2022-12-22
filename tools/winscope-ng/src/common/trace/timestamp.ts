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

export enum TimestampType {
  ELAPSED = "ELAPSED",
  REAL = "REAL",
}

export class Timestamp {
  constructor(type: TimestampType, valueNs: bigint) {
    this.type = type;
    this.valueNs = valueNs;
  }

  static from(timestampType: TimestampType, elapsedTimestamp: bigint, realToElapsedTimeOffsetNs: bigint | undefined = undefined) {
    switch (timestampType) {
    case TimestampType.REAL:
      if (realToElapsedTimeOffsetNs === undefined) {
        throw new Error("realToElapsedTimeOffsetNs can't be undefined to use real timestamp");
      }
      return new Timestamp(TimestampType.REAL, elapsedTimestamp + realToElapsedTimeOffsetNs);
      break;
    case TimestampType.ELAPSED:
      return new Timestamp(TimestampType.ELAPSED, elapsedTimestamp);
      break;
    default:
      throw new Error("Unhandled timestamp type");
    }
  }

  public getType(): TimestampType {
    return this.type;
  }

  public getValueNs(): bigint {
    return this.valueNs;
  }

  public valueOf(): bigint {
    return this.getValueNs();
  }

  private readonly type: TimestampType;
  private readonly valueNs: bigint;
}

export class RealTimestamp extends Timestamp {
  constructor(valueNs: bigint) {
    super(TimestampType.REAL, valueNs);
  }
}

export class ElapsedTimestamp extends Timestamp {
  constructor(valueNs: bigint) {
    super(TimestampType.ELAPSED, valueNs);
  }
}