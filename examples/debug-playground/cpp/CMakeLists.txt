cmake_minimum_required(VERSION 3.10)
project(debug_demo CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# The Debug view offers “cmake: demo” for this target.
add_executable(demo main.cpp)
