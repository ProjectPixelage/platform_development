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

import {Cuj, Event, Transition} from 'flickerlib/common';
import {LayerTraceEntry} from 'flickerlib/layers/LayerTraceEntry';
import {WindowManagerState} from 'flickerlib/windows/WindowManagerState';
import {LogMessage} from './protolog';
import {ScreenRecordingTraceEntry} from './screen_recording';

export enum TraceType {
  WINDOW_MANAGER,
  SURFACE_FLINGER,
  SCREEN_RECORDING,
  TRANSACTIONS,
  TRANSACTIONS_LEGACY,
  WAYLAND,
  WAYLAND_DUMP,
  PROTO_LOG,
  SYSTEM_UI,
  INPUT_METHOD_CLIENTS,
  INPUT_METHOD_MANAGER_SERVICE,
  INPUT_METHOD_SERVICE,
  EVENT_LOG,
  WM_TRANSITION,
  SHELL_TRANSITION,
  TRANSITION,
  CUJS,
  TAG,
  ERROR,
  TEST_TRACE_STRING,
  TEST_TRACE_NUMBER,
  VIEW_CAPTURE,
  VIEW_CAPTURE_LAUNCHER_ACTIVITY,
  VIEW_CAPTURE_TASKBAR_DRAG_LAYER,
  VIEW_CAPTURE_TASKBAR_OVERLAY_DRAG_LAYER,
}

// view capture types
export type ViewNode = any;
export type FrameData = any;

export interface TraceEntryTypeMap {
  [TraceType.PROTO_LOG]: LogMessage;
  [TraceType.SURFACE_FLINGER]: LayerTraceEntry;
  [TraceType.SCREEN_RECORDING]: ScreenRecordingTraceEntry;
  [TraceType.SYSTEM_UI]: object;
  [TraceType.TRANSACTIONS]: object;
  [TraceType.TRANSACTIONS_LEGACY]: object;
  [TraceType.WAYLAND]: object;
  [TraceType.WAYLAND_DUMP]: object;
  [TraceType.WINDOW_MANAGER]: WindowManagerState;
  [TraceType.INPUT_METHOD_CLIENTS]: object;
  [TraceType.INPUT_METHOD_MANAGER_SERVICE]: object;
  [TraceType.INPUT_METHOD_SERVICE]: object;
  [TraceType.EVENT_LOG]: Event;
  [TraceType.WM_TRANSITION]: object;
  [TraceType.SHELL_TRANSITION]: object;
  [TraceType.TRANSITION]: Transition;
  [TraceType.CUJS]: Cuj;
  [TraceType.TAG]: object;
  [TraceType.ERROR]: object;
  [TraceType.TEST_TRACE_STRING]: string;
  [TraceType.TEST_TRACE_NUMBER]: number;
  [TraceType.VIEW_CAPTURE]: object;
  [TraceType.VIEW_CAPTURE_LAUNCHER_ACTIVITY]: FrameData;
  [TraceType.VIEW_CAPTURE_TASKBAR_DRAG_LAYER]: FrameData;
  [TraceType.VIEW_CAPTURE_TASKBAR_OVERLAY_DRAG_LAYER]: FrameData;
}

export class TraceTypeUtils {
  private static UI_PIPELINE_ORDER = [
    TraceType.INPUT_METHOD_CLIENTS,
    TraceType.INPUT_METHOD_SERVICE,
    TraceType.INPUT_METHOD_MANAGER_SERVICE,
    TraceType.PROTO_LOG,
    TraceType.WINDOW_MANAGER,
    TraceType.TRANSACTIONS,
    TraceType.SURFACE_FLINGER,
    TraceType.SCREEN_RECORDING,
  ];

  private static DISPLAY_ORDER = [
    TraceType.SCREEN_RECORDING,
    TraceType.SURFACE_FLINGER,
    TraceType.WINDOW_MANAGER,
    TraceType.INPUT_METHOD_CLIENTS,
    TraceType.INPUT_METHOD_MANAGER_SERVICE,
    TraceType.INPUT_METHOD_SERVICE,
    TraceType.TRANSACTIONS,
    TraceType.TRANSACTIONS_LEGACY,
    TraceType.PROTO_LOG,
    TraceType.WM_TRANSITION,
    TraceType.SHELL_TRANSITION,
    TraceType.TRANSITION,
    TraceType.VIEW_CAPTURE,
    TraceType.VIEW_CAPTURE_LAUNCHER_ACTIVITY,
    TraceType.VIEW_CAPTURE_TASKBAR_DRAG_LAYER,
    TraceType.VIEW_CAPTURE_TASKBAR_OVERLAY_DRAG_LAYER,
  ];

  private static TRACES_WITH_VIEWERS = [
    TraceType.SCREEN_RECORDING,
    TraceType.SURFACE_FLINGER,
    TraceType.WINDOW_MANAGER,
    TraceType.INPUT_METHOD_CLIENTS,
    TraceType.INPUT_METHOD_MANAGER_SERVICE,
    TraceType.INPUT_METHOD_SERVICE,
    TraceType.TRANSACTIONS,
    TraceType.TRANSACTIONS_LEGACY,
    TraceType.PROTO_LOG,
    TraceType.TRANSITION,
    TraceType.VIEW_CAPTURE,
    TraceType.VIEW_CAPTURE_LAUNCHER_ACTIVITY,
    TraceType.VIEW_CAPTURE_TASKBAR_DRAG_LAYER,
    TraceType.VIEW_CAPTURE_TASKBAR_OVERLAY_DRAG_LAYER,
  ];

  static isTraceTypeWithViewer(t: TraceType): boolean {
    return TraceTypeUtils.TRACES_WITH_VIEWERS.includes(t);
  }

  static compareByUiPipelineOrder(t: TraceType, u: TraceType) {
    const tIndex = TraceTypeUtils.findIndexInOrder(t, TraceTypeUtils.UI_PIPELINE_ORDER);
    const uIndex = TraceTypeUtils.findIndexInOrder(u, TraceTypeUtils.UI_PIPELINE_ORDER);
    return tIndex >= 0 && uIndex >= 0 && tIndex < uIndex;
  }

  static compareByDisplayOrder(t: TraceType, u: TraceType) {
    const tIndex = TraceTypeUtils.findIndexInOrder(t, TraceTypeUtils.DISPLAY_ORDER);
    const uIndex = TraceTypeUtils.findIndexInOrder(u, TraceTypeUtils.DISPLAY_ORDER);
    return tIndex - uIndex;
  }

  private static findIndexInOrder(traceType: TraceType, order: TraceType[]): number {
    return order.findIndex((type) => {
      return type === traceType;
    });
  }
}
