/*
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#pragma once

#ifndef _WIN32

// Implementation of the NFSv3 protocol as described in:
// https://tools.ietf.org/html/rfc1813

#include "eden/fs/nfs/NfsDispatcher.h"
#include "eden/fs/nfs/rpc/Server.h"
#include "eden/fs/utils/ProcessAccessLog.h"

namespace folly {
class Executor;
}

namespace facebook::eden {

class Notifications;
class ProcessNameCache;

class Nfsd3 {
 public:
  /**
   * Create a new RPC NFSv3 program.
   *
   * If registerWithRpcbind is set, this NFSv3 program will advertise itself
   * against the rpcbind daemon allowing it to be visible system wide. Be aware
   * that for a given transport (tcp/udp) only one NFSv3 program can be
   * registered with rpcbind, and thus if a real NFS server is running on this
   * host, EdenFS won't be able to register itself.
   *
   * All the socket processing will be run on the EventBase passed in. This
   * also must be called on that EventBase thread.
   *
   * Note: at mount time, EdenFS will manually call mount.nfs with -o port
   * to manually specify the port on which this server is bound, so registering
   * is not necessary for a properly behaving EdenFS.
   */
  Nfsd3(
      bool registerWithRpcbind,
      folly::EventBase* evb,
      std::shared_ptr<folly::Executor> threadPool,
      std::unique_ptr<NfsDispatcher> dispatcher,
      const folly::Logger* straceLogger,
      std::shared_ptr<ProcessNameCache> processNameCache,
      folly::Duration requestTimeout,
      Notifications* FOLLY_NULLABLE notifications,
      bool caseSensitive);

  ~Nfsd3();

  /**
   * Obtain the TCP port that this NFSv3 program is listening on.
   */
  uint16_t getPort() const {
    return server_.getPort();
  }

  struct StopData {};

  /**
   * Return a future that will be triggered on unmount.
   */
  folly::SemiFuture<StopData> getStopFuture();

  ProcessAccessLog& getProcessAccessLog() {
    return processAccessLog_;
  }

  Nfsd3(const Nfsd3&) = delete;
  Nfsd3(Nfsd3&&) = delete;
  Nfsd3& operator=(const Nfsd3&) = delete;
  Nfsd3& operator=(Nfsd3&&) = delete;

 private:
  RpcServer server_;
  ProcessAccessLog processAccessLog_;
  folly::Promise<StopData> stopPromise_;
};

} // namespace facebook::eden

#endif
