export type Credentials = {
  apiKey: string;
  hasConsented: boolean | "false" | "true" /* mistakes were made */;
};
export const DEFAULT_CREDENTIALS: Credentials = {
  apiKey: "",
  hasConsented: false,
};

export type Preferences = {
  startRecordingKey: string;
  stopRecordingKey: string;
  unreliableConnection: boolean;
};
export const DEFAULT_PREFERENCES: Preferences = {
  startRecordingKey: "f4",
  stopRecordingKey: "f5",
  unreliableConnection: false,
};

export type IpcResponse<T> =
  | { success: true; data: T }
  | { success: false; error: string };

export const unwrapIpcResponse = <T>(response: IpcResponse<T>): T => {
  if (response.success) {
    return response.data;
  }
  throw new Error(response.error);
};
