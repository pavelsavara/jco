export namespace WasiSocketsTcp {
  /**
   * Bind the socket to a specific network on the provided IP address and port.
   * 
   * If the IP address is zero (`0.0.0.0` in IPv4, `::` in IPv6), it is left to the implementation to decide which
   * network interface(s) to bind to.
   * If the TCP/UDP port is zero, the socket will be bound to a random free port.
   * 
   * When a socket is not explicitly bound, the first invocation to a listen or connect operation will
   * implicitly bind the socket.
   * 
   * Unlike in POSIX, this function is async. This enables interactive WASI hosts to inject permission prompts.
   * 
   * # Typical `start` errors
   * - `address-family-mismatch`:   The `local-address` has the wrong address family. (EINVAL)
   * - `already-bound`:             The socket is already bound. (EINVAL)
   * - `concurrency-conflict`:      Another `bind`, `connect` or `listen` operation is already in progress. (EALREADY)
   * 
   * # Typical `finish` errors
   * - `ephemeral-ports-exhausted`: No ephemeral ports available. (EADDRINUSE, ENOBUFS on Windows)
   * - `address-in-use`:            Address is already in use. (EADDRINUSE)
   * - `address-not-bindable`:      `local-address` is not an address that the `network` can bind to. (EADDRNOTAVAIL)
   * - `not-in-progress`:           A `bind` operation is not in progress.
   * - `would-block`:               Can't finish the operation, it is still in progress. (EWOULDBLOCK, EAGAIN)
   * 
   * # References
   * - <https://pubs.opengroup.org/onlinepubs/9699919799/functions/bind.html>
   * - <https://man7.org/linux/man-pages/man2/bind.2.html>
   * - <https://learn.microsoft.com/en-us/windows/win32/api/winsock/nf-winsock-bind>
   * - <https://man.freebsd.org/cgi/man.cgi?query=bind&sektion=2&format=html>
   */
  export function startBind(this: TcpSocket, network: Network, localAddress: IpSocketAddress): void;
  export function finishBind(this: TcpSocket): void;
  /**
   * Connect to a remote endpoint.
   * 
   * On success:
   * - the socket is transitioned into the Connection state
   * - a pair of streams is returned that can be used to read & write to the connection
   * 
   * # Typical `start` errors
   * - `address-family-mismatch`:   The `remote-address` has the wrong address family. (EAFNOSUPPORT)
   * - `invalid-remote-address`:    The IP address in `remote-address` is set to INADDR_ANY (`0.0.0.0` / `::`). (EADDRNOTAVAIL on Windows)
   * - `invalid-remote-address`:    The port in `remote-address` is set to 0. (EADDRNOTAVAIL on Windows)
   * - `already-attached`:          The socket is already attached to a different network. The `network` passed to `connect` must be identical to the one passed to `bind`.
   * - `already-connected`:         The socket is already in the Connection state. (EISCONN)
   * - `already-listening`:         The socket is already in the Listener state. (EOPNOTSUPP, EINVAL on Windows)
   * - `concurrency-conflict`:      Another `bind`, `connect` or `listen` operation is already in progress. (EALREADY)
   * 
   * # Typical `finish` errors
   * - `timeout`:                   Connection timed out. (ETIMEDOUT)
   * - `connection-refused`:        The connection was forcefully rejected. (ECONNREFUSED)
   * - `connection-reset`:          The connection was reset. (ECONNRESET)
   * - `remote-unreachable`:        The remote address is not reachable. (EHOSTUNREACH, EHOSTDOWN, ENETUNREACH, ENETDOWN)
   * - `ephemeral-ports-exhausted`: Tried to perform an implicit bind, but there were no ephemeral ports available. (EADDRINUSE, EADDRNOTAVAIL on Linux, EAGAIN on BSD)
   * - `not-in-progress`:           A `connect` operation is not in progress.
   * - `would-block`:               Can't finish the operation, it is still in progress. (EWOULDBLOCK, EAGAIN)
   * 
   * # References
   * - <https://pubs.opengroup.org/onlinepubs/9699919799/functions/connect.html>
   * - <https://man7.org/linux/man-pages/man2/connect.2.html>
   * - <https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-connect>
   * - <https://man.freebsd.org/cgi/man.cgi?connect>
   */
  export function startConnect(this: TcpSocket, network: Network, remoteAddress: IpSocketAddress): void;
  export function finishConnect(this: TcpSocket): [InputStream, OutputStream];
  /**
   * Start listening for new connections.
   * 
   * Transitions the socket into the Listener state.
   * 
   * Unlike POSIX:
   * - this function is async. This enables interactive WASI hosts to inject permission prompts.
   * - the socket must already be explicitly bound.
   * 
   * # Typical `start` errors
   * - `not-bound`:                 The socket is not bound to any local address. (EDESTADDRREQ)
   * - `already-connected`:         The socket is already in the Connection state. (EISCONN, EINVAL on BSD)
   * - `already-listening`:         The socket is already in the Listener state.
   * - `concurrency-conflict`:      Another `bind`, `connect` or `listen` operation is already in progress. (EINVAL on BSD)
   * 
   * # Typical `finish` errors
   * - `ephemeral-ports-exhausted`: Tried to perform an implicit bind, but there were no ephemeral ports available. (EADDRINUSE)
   * - `not-in-progress`:           A `listen` operation is not in progress.
   * - `would-block`:               Can't finish the operation, it is still in progress. (EWOULDBLOCK, EAGAIN)
   * 
   * # References
   * - <https://pubs.opengroup.org/onlinepubs/9699919799/functions/listen.html>
   * - <https://man7.org/linux/man-pages/man2/listen.2.html>
   * - <https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-listen>
   * - <https://man.freebsd.org/cgi/man.cgi?query=listen&sektion=2>
   */
  export function startListen(this: TcpSocket): void;
  export function finishListen(this: TcpSocket): void;
  /**
   * Accept a new client socket.
   * 
   * The returned socket is bound and in the Connection state.
   * 
   * On success, this function returns the newly accepted client socket along with
   * a pair of streams that can be used to read & write to the connection.
   * 
   * # Typical errors
   * - `not-listening`: Socket is not in the Listener state. (EINVAL)
   * - `would-block`:   No pending connections at the moment. (EWOULDBLOCK, EAGAIN)
   * 
   * Host implementations must skip over transient errors returned by the native accept syscall.
   * 
   * # References
   * - <https://pubs.opengroup.org/onlinepubs/9699919799/functions/accept.html>
   * - <https://man7.org/linux/man-pages/man2/accept.2.html>
   * - <https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept>
   * - <https://man.freebsd.org/cgi/man.cgi?query=accept&sektion=2>
   */
  export function accept(this: TcpSocket): [TcpSocket, InputStream, OutputStream];
  /**
   * Get the bound local address.
   * 
   * # Typical errors
   * - `not-bound`: The socket is not bound to any local address.
   * 
   * # References
   * - <https://pubs.opengroup.org/onlinepubs/9699919799/functions/getsockname.html>
   * - <https://man7.org/linux/man-pages/man2/getsockname.2.html>
   * - <https://learn.microsoft.com/en-us/windows/win32/api/winsock/nf-winsock-getsockname>
   * - <https://man.freebsd.org/cgi/man.cgi?getsockname>
   */
  export function localAddress(this: TcpSocket): IpSocketAddress;
  /**
   * Get the bound remote address.
   * 
   * # Typical errors
   * - `not-connected`: The socket is not connected to a remote address. (ENOTCONN)
   * 
   * # References
   * - <https://pubs.opengroup.org/onlinepubs/9699919799/functions/getpeername.html>
   * - <https://man7.org/linux/man-pages/man2/getpeername.2.html>
   * - <https://learn.microsoft.com/en-us/windows/win32/api/winsock/nf-winsock-getpeername>
   * - <https://man.freebsd.org/cgi/man.cgi?query=getpeername&sektion=2&n=1>
   */
  export function remoteAddress(this: TcpSocket): IpSocketAddress;
  /**
   * Whether this is a IPv4 or IPv6 socket.
   * 
   * Equivalent to the SO_DOMAIN socket option.
   */
  export function addressFamily(this: TcpSocket): IpAddressFamily;
  /**
   * Whether IPv4 compatibility (dual-stack) mode is disabled or not.
   * 
   * Equivalent to the IPV6_V6ONLY socket option.
   * 
   * # Typical errors
   * - `ipv6-only-operation`:  (get/set) `this` socket is an IPv4 socket.
   * - `already-bound`:        (set) The socket is already bound.
   * - `not-supported`:        (set) Host does not support dual-stack sockets. (Implementations are not required to.)
   * - `concurrency-conflict`: (set) A `bind`, `connect` or `listen` operation is already in progress. (EALREADY)
   */
  export function ipv6Only(this: TcpSocket): boolean;
  export function setIpv6Only(this: TcpSocket, value: boolean): void;
  /**
   * Hints the desired listen queue size. Implementations are free to ignore this.
   * 
   * # Typical errors
   * - `already-connected`:    (set) The socket is already in the Connection state.
   * - `concurrency-conflict`: (set) A `bind`, `connect` or `listen` operation is already in progress. (EALREADY)
   */
  export function setListenBacklogSize(this: TcpSocket, value: bigint): void;
  /**
   * Equivalent to the SO_KEEPALIVE socket option.
   * 
   * # Typical errors
   * - `concurrency-conflict`: (set) A `bind`, `connect` or `listen` operation is already in progress. (EALREADY)
   */
  export function keepAlive(this: TcpSocket): boolean;
  export function setKeepAlive(this: TcpSocket, value: boolean): void;
  /**
   * Equivalent to the TCP_NODELAY socket option.
   * 
   * # Typical errors
   * - `concurrency-conflict`: (set) A `bind`, `connect` or `listen` operation is already in progress. (EALREADY)
   */
  export function noDelay(this: TcpSocket): boolean;
  export function setNoDelay(this: TcpSocket, value: boolean): void;
  /**
   * Equivalent to the IP_TTL & IPV6_UNICAST_HOPS socket options.
   * 
   * # Typical errors
   * - `already-connected`:    (set) The socket is already in the Connection state.
   * - `already-listening`:    (set) The socket is already in the Listener state.
   * - `concurrency-conflict`: (set) A `bind`, `connect` or `listen` operation is already in progress. (EALREADY)
   */
  export function unicastHopLimit(this: TcpSocket): number;
  export function setUnicastHopLimit(this: TcpSocket, value: number): void;
  /**
   * The kernel buffer space reserved for sends/receives on this socket.
   * 
   * Note #1: an implementation may choose to cap or round the buffer size when setting the value.
   * In other words, after setting a value, reading the same setting back may return a different value.
   * 
   * Note #2: there is not necessarily a direct relationship between the kernel buffer size and the bytes of
   * actual data to be sent/received by the application, because the kernel might also use the buffer space
   * for internal metadata structures.
   * 
   * Equivalent to the SO_RCVBUF and SO_SNDBUF socket options.
   * 
   * # Typical errors
   * - `already-connected`:    (set) The socket is already in the Connection state.
   * - `already-listening`:    (set) The socket is already in the Listener state.
   * - `concurrency-conflict`: (set) A `bind`, `connect` or `listen` operation is already in progress. (EALREADY)
   */
  export function receiveBufferSize(this: TcpSocket): bigint;
  export function setReceiveBufferSize(this: TcpSocket, value: bigint): void;
  export function sendBufferSize(this: TcpSocket): bigint;
  export function setSendBufferSize(this: TcpSocket, value: bigint): void;
  /**
   * Create a `pollable` which will resolve once the socket is ready for I/O.
   * 
   * Note: this function is here for WASI Preview2 only.
   * It's planned to be removed when `future` is natively supported in Preview3.
   */
  export function subscribe(this: TcpSocket): Pollable;
  /**
   * Initiate a graceful shutdown.
   * 
   * - receive: the socket is not expecting to receive any more data from the peer. All subsequent read
   * operations on the `input-stream` associated with this socket will return an End Of Stream indication.
   * Any data still in the receive queue at time of calling `shutdown` will be discarded.
   * - send: the socket is not expecting to send any more data to the peer. All subsequent write
   * operations on the `output-stream` associated with this socket will return an error.
   * - both: same effect as receive & send combined.
   * 
   * The shutdown function does not close (drop) the socket.
   * 
   * # Typical errors
   * - `not-connected`: The socket is not in the Connection state. (ENOTCONN)
   * 
   * # References
   * - <https://pubs.opengroup.org/onlinepubs/9699919799/functions/shutdown.html>
   * - <https://man7.org/linux/man-pages/man2/shutdown.2.html>
   * - <https://learn.microsoft.com/en-us/windows/win32/api/winsock/nf-winsock-shutdown>
   * - <https://man.freebsd.org/cgi/man.cgi?query=shutdown&sektion=2>
   */
  export function shutdown(this: TcpSocket, shutdownType: ShutdownType): void;
  /**
   * Dispose of the specified `tcp-socket`, after which it may no longer be used.
   * 
   * Similar to the POSIX `close` function.
   * 
   * Note: this function is scheduled to be removed when Resources are natively supported in Wit.
   */
  export function dropTcpSocket(this: TcpSocket): void;
}
import type { InputStream } from '../exports/wasi-io-streams';
export { InputStream };
import type { OutputStream } from '../exports/wasi-io-streams';
export { OutputStream };
import type { Pollable } from '../exports/wasi-poll-poll';
export { Pollable };
import type { Network } from '../exports/wasi-sockets-network';
export { Network };
import type { ErrorCode } from '../exports/wasi-sockets-network';
export { ErrorCode };
import type { IpSocketAddress } from '../exports/wasi-sockets-network';
export { IpSocketAddress };
import type { IpAddressFamily } from '../exports/wasi-sockets-network';
export { IpAddressFamily };
/**
 * A TCP socket handle.
 */
export type TcpSocket = number;
/**
 * # Variants
 * 
 * ## `"receive"`
 * 
 * Similar to `SHUT_RD` in POSIX.
 * ## `"send"`
 * 
 * Similar to `SHUT_WR` in POSIX.
 * ## `"both"`
 * 
 * Similar to `SHUT_RDWR` in POSIX.
 */
export type ShutdownType = 'receive' | 'send' | 'both';
