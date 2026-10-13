import ExampleCore
import Observation

@MainActor @Observable final class AppGraph {
 let routes = RouteOwner()
 func navigate(_ route: Route) { routes.show(route) }
}
