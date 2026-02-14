import { configureStore } from "@reduxjs/toolkit";
import vmReducer from "./vmSlice";
import uiReducer from "./uiSlice";

const store = configureStore({
  reducer: {
    vm: vmReducer,
    ui: uiReducer,
  },
  middleware: (getDefaultMiddleware) =>
    getDefaultMiddleware({
      serializableCheck: {
        ignoredPaths: ['vm.debugMessages', 'vm.timeoutHandles'],
        ignoredActions: ['vm/debugContentAdded', 'vm/setTimeoutHandle'],
      },
    }),
});
// Infer the `RootState` and `AppDispatch` types from the store itself
export type RootState = ReturnType<typeof store.getState>
// Inferred type: {posts: PostsState, comments: CommentsState, users: UsersState}
export type AppDispatch = typeof store.dispatch

export default store;
